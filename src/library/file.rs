use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::mpsc::channel,
    thread,
};

use crate::{
    Exit, IntoAsyncActor,
    actor::Signal,
    global::{Context, pid, send, send_signal, spawn_linked},
    io::FilledBuffer,
    receive,
};

fn file_actor(path: impl Into<PathBuf>) -> impl IntoAsyncActor {
    let owner = pid();
    let path = path.into();

    async move || {
        let ctx = Context::new();
        let (tx, rx) = channel();

        thread::spawn(move || {
            let mut file = match File::open(path) {
                Ok(file) => file,
                Err(err) => {
                    ctx.exit(Exit::Io(err.to_string(), err.kind()));
                    return;
                }
            };

            for msg in rx {
                match msg {
                    FileRequest::Read { offset, len } => {
                        let len = len.max(CHUNK_SIZE);
                        let mut data = vec![0; CHUNK_SIZE];

                        match file.seek(SeekFrom::Start(offset)) {
                            Ok(_) => {}
                            Err(err) => {
                                ctx.exit(Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }

                        match file.read(&mut data[..len]) {
                            Ok(n) => {
                                let _ = ctx.send(
                                    owner,
                                    FileReply::Read(FilledBuffer::new(data.into_boxed_slice(), n)),
                                );
                            }
                            Err(err) => {
                                ctx.exit(Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }
                    }
                    FileRequest::Write { offset, len, data } => {
                        match file.seek(SeekFrom::Start(offset)) {
                            Ok(_) => {}
                            Err(err) => {
                                ctx.exit(Exit::Io(err.to_string(), err.kind()));
                                return;
                            }
                        }

                        match file.write_all(&data[..len]) {
                            Ok(_) => {}
                            Err(err) => {
                                ctx.exit(Exit::Io(err.to_string(), err.kind()));
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
    Read(FilledBuffer),
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
        );

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

    // TODO: Close port better.
    send_signal(port, Signal::Exit(port, Exit::Normal));

    String::from_utf8(buffer).map_err(|_| ReadStringError::InvalidUtf8)
}
