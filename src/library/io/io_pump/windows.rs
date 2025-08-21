use std::{
    collections::HashMap,
    io::Error,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
    sync::Arc,
};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_IO_PENDING, FALSE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, TRUE,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
        OPEN_EXISTING, ReadFile, WriteFile,
    },
    System::{
        IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED},
        Threading::INFINITE,
    },
};

use crate::{
    Exit, Pid,
    global::{
        send,
        sync::{pid, register},
    },
    library::io::{
        buffer_pool::Buffer,
        io_pump::{
            CloseRequest, ErrorResponse, OpenRequest, OpenResponse, ReadRequest, ReadResponse,
            WriteRequest, WriteResponse,
        },
    },
    receive,
};

fn encode_wide(path: &Path) -> Vec<u16> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    wide
}

fn get_error() -> Error {
    let code = unsafe { GetLastError() };
    Error::from_raw_os_error(code as _)
}

pub struct CompletionPort {
    handle: HANDLE,
}

unsafe impl Send for CompletionPort {}
unsafe impl Sync for CompletionPort {}

impl CompletionPort {
    pub fn new() -> Self {
        let handle = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 0) };

        if handle == null_mut() {
            let error = get_error();
            panic!("Failed to create completion port {}", error);
        }

        Self { handle }
    }

    pub fn open_file(&self, path: impl AsRef<Path>) -> Result<OpenDescriptor, Error> {
        let path = encode_wide(path.as_ref());

        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };

        if handle == null_mut() {
            return Err(get_error());
        }

        let iocp = unsafe { CreateIoCompletionPort(handle, self.handle, 0, 0) };

        if iocp == null_mut() {
            return Err(get_error());
        }

        Ok(OpenDescriptor(handle))
    }

    fn pump(&self) -> Box<ActiveOperation> {
        let mut bytes_transferred = 0;
        let mut completion_key = 0;
        let mut overlapped = null_mut();
        let success = unsafe {
            GetQueuedCompletionStatus(
                self.handle,
                &mut bytes_transferred,
                &mut completion_key,
                &mut overlapped,
                INFINITE,
            )
        };

        if success == FALSE {
            let error = unsafe { GetLastError() };
            panic!("Pump failed {}", error);
        }

        let mut operation = unsafe { Box::from_raw(overlapped as *mut ActiveOperation) };

        operation.start = operation.start.wrapping_add(bytes_transferred as _);
        operation.length -= bytes_transferred as usize;

        unsafe {
            operation
                .buffer
                .set_len(operation.buffer.len() + bytes_transferred as usize);
        }

        operation
    }
}

impl Drop for CompletionPort {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub enum Operation {
    Read,
    Write,
    Accept,
}

fn read(mut request: ReadRequest) -> Result<(), Error> {
    request.buffer.resize(0);

    let operation = Box::new(ActiveOperation::from(request));

    let success = unsafe {
        ReadFile(
            operation.descriptor.0,
            operation.start,
            operation.length as _,
            null_mut(),
            Box::into_raw(operation).cast(),
        )
    };

    let error = unsafe { GetLastError() };
    if success == FALSE && error != ERROR_IO_PENDING {
        return Err(get_error());
    }

    if success == TRUE {
        return Err(Error::other("Don't know what to do now!"));
    }

    Ok(())
}

fn write(request: WriteRequest) -> Result<(), Error> {
    let mut operation = Box::new(ActiveOperation::from(request));
    operation.buffer.resize(0);

    resume_write(operation)
}

fn resume_write(operation: Box<ActiveOperation>) -> Result<(), Error> {
    let success = unsafe {
        WriteFile(
            operation.descriptor.0,
            operation.start,
            operation.length as _,
            null_mut(),
            Box::into_raw(operation).cast(),
        )
    };

    let error = unsafe { GetLastError() };
    if success == FALSE && error != ERROR_IO_PENDING {
        return Err(get_error());
    }

    if success == TRUE {
        return Err(Error::other("Don't know what to do now!"));
    }

    Ok(())
}

#[repr(C)]
pub struct ActiveOperation {
    // SAFETY: It is very important that overlapped comes first!
    overlapped: OVERLAPPED,

    buffer: Buffer,
    start: *mut u8,
    length: usize,
    pid: Pid,
    descriptor: Descriptor,
    operation: Operation,
}

impl From<ReadRequest> for ActiveOperation {
    fn from(mut value: ReadRequest) -> Self {
        let mut overlapped: OVERLAPPED = Default::default();
        overlapped.Anonymous.Anonymous.Offset = value.offset as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (value.offset >> 32) as u32;

        Self {
            overlapped,
            start: value.buffer.as_mut_ptr(),
            length: value.buffer.capacity(),
            buffer: value.buffer,
            pid: value.pid,
            descriptor: value.descriptor,
            operation: Operation::Read,
        }
    }
}

impl From<WriteRequest> for ActiveOperation {
    fn from(mut value: WriteRequest) -> Self {
        let mut overlapped: OVERLAPPED = Default::default();
        overlapped.Anonymous.Anonymous.Offset = value.offset as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (value.offset >> 32) as u32;

        Self {
            overlapped,
            start: value.buffer.as_mut_ptr(),
            length: value.buffer.len(),
            buffer: value.buffer,
            pid: value.pid,
            descriptor: value.descriptor,
            operation: Operation::Write,
        }
    }
}

// TODO: Not Currently used yet
#[derive(Copy, Clone, PartialEq, Hash, Eq)]
pub struct Descriptor(HANDLE);

unsafe impl Send for Descriptor {}
unsafe impl Sync for Descriptor {}

pub struct OpenDescriptor(HANDLE);

unsafe impl Send for OpenDescriptor {}

impl Drop for OpenDescriptor {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub async fn pump_actor() -> Exit {
    register("io_pump", pid());

    let mut descriptors = HashMap::<Descriptor, OpenDescriptor>::new();
    let port = Arc::new(CompletionPort::new());

    // TODO: Spawn more than 1 and move to their own actors
    {
        let port = port.clone();
        crate::thread::spawn(move || {
            loop {
                let operation = port.pump();

                match operation.operation {
                    Operation::Read => {
                        crate::global::sync::send(
                            operation.pid,
                            ReadResponse {
                                buffer: operation.buffer,
                            },
                        );
                    }
                    Operation::Write => {
                        if operation.length == 0 {
                            crate::global::sync::send(
                                operation.pid,
                                WriteResponse {
                                    buffer: operation.buffer,
                                },
                            );
                        } else {
                            resume_write(operation).unwrap();
                        }
                    }
                    Operation::Accept => todo!(),
                }
            }
        });
    }

    loop {
        receive! {
            match OpenRequest {
                req => {
                    handle_errors(req.pid, async || {
                        let open_descriptor = port.open_file(req.path)?; // TODO: HANDLE ME
                        let descriptor = Descriptor(open_descriptor.0);
                        descriptors.insert(descriptor, open_descriptor);

                        send(req.pid, OpenResponse {
                            descriptor,
                        }).await;

                        Ok(())
                    }).await;
                }
            }
            match CloseRequest {
                req => {
                    let _ = descriptors.remove(&req.descriptor);
                }
            }
            match ReadRequest {
                req => {
                    handle_errors(req.pid, async || read(req)).await;
                }
            }
            match WriteRequest {
                req => {
                    handle_errors(req.pid, async || write(req)).await
                }
            }
        }
    }
}

// TODO: Figure out a better way of handling errors.
async fn handle_errors<F, FT>(pid: Pid, f: F)
where
    F: FnOnce() -> FT,
    FT: Future<Output = Result<(), Error>>,
{
    if let Err(err) = f().await {
        send(pid, ErrorResponse { error: err }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn read_test() {
        let port = CompletionPort::new();
        let open_file = port.open_file("Cargo.toml").unwrap();
        let file = Descriptor(open_file.0);
        let request = ReadRequest {
            buffer: Buffer::new(),
            descriptor: file,
            offset: 0,
            pid: Pid::invalid(),
        };

        read(request).unwrap();
        let operation = port.pump();
        let buf = std::str::from_utf8(&operation.buffer).unwrap();
        println!("{}", buf);
    }
}
