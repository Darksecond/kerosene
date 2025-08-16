use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::mpsc::channel,
};

use crate::{
    Exit, IntoAsyncActor,
    global::{
        exit, send, spawn_linked,
        sync::{self, pid},
    },
    library::io::buffer_pool::Buffer,
    receive,
};

fn file_actor(path: impl Into<PathBuf>) -> impl IntoAsyncActor {
    let owner = pid();
    let path = path.into();

    async move || {
        let pid = pid();
        let (tx, rx) = channel();

        crate::thread::spawn(move || {
            let mut file = match File::open(path) {
                Ok(file) => file,
                Err(err) => {
                    sync::exit(pid, Exit::Io(err.to_string(), err.kind()));
                    return;
                }
            };

            for msg in rx {
                match msg {
                    FileRequest::Read { offset, len } => {
                        let mut buffer = Buffer::new();
                        buffer.resize(len);

                        match file.seek(SeekFrom::Start(offset)) {
                            Ok(_) => {}
                            Err(err) => {
                                sync::exit(pid, Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }

                        match file.read(&mut buffer) {
                            Ok(n) => {
                                buffer.resize(n);
                                sync::send(owner, FileReply::Read(buffer));
                            }
                            Err(err) => {
                                sync::exit(pid, Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }
                    }
                    FileRequest::Write { offset, len, data } => {
                        match file.seek(SeekFrom::Start(offset)) {
                            Ok(_) => {}
                            Err(err) => {
                                sync::exit(pid, Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }

                        match file.write_all(&data[..len]) {
                            Ok(_) => {}
                            Err(err) => {
                                sync::exit(pid, Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }
                    }
                }
            }
        });

        loop {
            receive! {
                match FileRequest {
                    request => {
                        tx.send(request).expect("Failed to send request to helper thread");
                    },
                }
            }
        }
    }
}

const CHUNK_SIZE: usize = 0x1000;

// TODO: Split up in ReadRequest and WriteRequest now that we use actors instead of ports.
pub enum FileRequest {
    Read {
        offset: u64,
        len: usize,
    },
    Write {
        offset: u64,
        len: usize,
        data: Box<[u8]>,
    },
}

pub enum FileReply {
    Write(usize),
    Read(Buffer),
}

pub enum ReadStringError {
    InvalidUtf8,
}

pub async fn read_string(path: impl Into<PathBuf>) -> Result<String, ReadStringError> {
    let port = spawn_linked(file_actor(path));

    let mut offset = 0;
    let mut buffer = Vec::new();

    loop {
        send(
            port,
            FileRequest::Read {
                offset: offset,
                len: CHUNK_SIZE,
            },
        )
        .await;

        receive! {
            match FileReply {
                FileReply::Read(read_buffer) => {
                    buffer.extend_from_slice(&read_buffer);
                    offset += read_buffer.len() as u64;

                    if read_buffer.len() == 0 {
                        break;
                    }
                }
            }
            // TODO: Optional Timeout
        }
    }

    exit(port, Exit::Normal).await;

    String::from_utf8(buffer).map_err(|_| ReadStringError::InvalidUtf8)
}
