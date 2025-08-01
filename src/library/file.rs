use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Sender, channel},
    },
};

use crate::{
    Exit,
    global::{self, send_port},
    io::FilledBuffer,
    port::{Port, PortContext},
    receive,
};

const CHUNK_SIZE: usize = 0x1000;

pub struct FilePort {
    path: Option<PathBuf>,
    tx: Option<Sender<FileRequest>>,
}

impl FilePort {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FilePort {
            path: Some(path.into()),
            tx: None,
        }
    }
}

impl Port for FilePort {
    type Message = FileRequest;

    fn start(&mut self, ctx: &Arc<PortContext>) {
        let ctx = ctx.clone();
        let (tx, rx) = channel();
        self.tx = Some(tx);

        let path = self.path.take().expect("Path not set");
        std::thread::spawn(move || {
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
                                let _ = ctx.send(FileReply::Read(FilledBuffer::new(
                                    data.into_boxed_slice(),
                                    n,
                                )));
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
    }

    fn stop(&mut self, _ctx: &Arc<PortContext>) {
        drop(self.tx.take());
    }

    fn receive(&mut self, _ctx: &Arc<PortContext>, message: Self::Message) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(message);
        }
    }
}

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
    let port = global::create_port(FilePort::new(path));

    let mut offset = 0;
    let mut buffer = Vec::new();

    loop {
        send_port(
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

    String::from_utf8(buffer).map_err(|_| ReadStringError::InvalidUtf8)
}
