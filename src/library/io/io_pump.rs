#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::Descriptor;

#[cfg(windows)]
pub use windows::pump_actor as pump;

use crate::Pid;
use crate::global::exit;
use crate::global::send;
use crate::global::sync;
use crate::global::sync::pid;
use crate::receive;

use super::buffer_pool::Buffer;
use std::path::PathBuf;

struct OpenRequest {
    pid: Pid,
    path: PathBuf,
}

struct OpenResponse {
    descriptor: Descriptor,
}

struct CloseRequest {
    descriptor: Descriptor,
}

struct ReadRequest {
    pid: Pid,
    descriptor: Descriptor,
    buffer: Buffer,
    offset: u64,
}

struct ReadResponse {
    buffer: Buffer,
}

struct WriteRequest {
    pid: Pid,
    descriptor: Descriptor,
    buffer: Buffer,
    offset: u64,
}

struct WriteResponse {
    buffer: Buffer,
}

struct ErrorResponse {
    error: std::io::Error,
}

pub async fn open_file(path: impl Into<PathBuf>) -> Descriptor {
    send(
        "io_pump",
        OpenRequest {
            pid: pid(),
            path: path.into(),
        },
    )
    .await;

    receive! {
        match OpenResponse {
            OpenResponse { descriptor } => descriptor,
        }
        match ErrorResponse {
            ErrorResponse { error } => {
                exit(pid(), error.into()).await;
                unreachable!()
            }
        }
    }
}

pub fn close_descriptor(descriptor: Descriptor) {
    sync::send("io_pump", CloseRequest { descriptor });
}

pub async fn read(descriptor: Descriptor, offset: u64, buffer: Buffer) -> Buffer {
    send(
        "io_pump",
        ReadRequest {
            pid: pid(),
            descriptor,
            buffer,
            offset,
        },
    )
    .await;

    receive! {
        match ReadResponse {
            ReadResponse { buffer } => buffer,
        }
        match ErrorResponse {
            ErrorResponse { error } => {
                exit(pid(), error.into()).await;
                unreachable!()
            }
        }
    }
}

pub async fn write(descriptor: Descriptor, offset: u64, buffer: Buffer) -> Buffer {
    send(
        "io_pump",
        WriteRequest {
            pid: pid(),
            descriptor,
            buffer,
            offset,
        },
    )
    .await;

    receive! {
        match WriteResponse {
            WriteResponse { buffer } => buffer,
        }
        match ErrorResponse {
            ErrorResponse { error } => {
                exit(pid(), error.into()).await;
                unreachable!()
            }
        }
    }
}

// TODO: This might need to be slightly redesigned because apparently `AcceptEx` uses a buffer to capture local and remote addresses?
pub async fn accept(listener: Descriptor) -> Descriptor {
    let _ = listener;
    todo!();
}
