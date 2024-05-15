//! Syscall handlers
//!

#![allow(dead_code)]

/* -------------------------------------------------------------------------- */
/*                               SYSCALL NUMBER                               */
/* -------------------------------------------------------------------------- */

use core::slice::{from_raw_parts, from_raw_parts_mut};

use alloc::{string::String, vec::Vec};

use crate::{
    fs::{
        disk::{Path, DISKFS},
        FileSys,
    },
    io::{Read, Seek, SeekFrom, Write},
    sbi::{console_getchar, shutdown},
    thread::current,
    userproc::{execute, exit, wait},
    OsError,
};

const SYS_HALT: usize = 1;
const SYS_EXIT: usize = 2;
const SYS_EXEC: usize = 3;
const SYS_WAIT: usize = 4;
const SYS_REMOVE: usize = 5;
const SYS_OPEN: usize = 6;
const SYS_READ: usize = 7;
const SYS_WRITE: usize = 8;
const SYS_SEEK: usize = 9;
const SYS_TELL: usize = 10;
const SYS_CLOSE: usize = 11;
const SYS_FSTAT: usize = 12;

pub fn syscall_handler(_id: usize, _args: [usize; 3]) -> isize {
    match _id {
        SYS_HALT => halt(),
        SYS_EXIT => exit(_args[0] as isize),
        SYS_EXEC => raw_execute_handler(_args),
        SYS_WAIT => wait(_args[0] as isize).unwrap_or(-1),
        SYS_OPEN => open(_args[0], _args[1]),
        SYS_CLOSE => close(_args[0]),
        SYS_READ => read(_args[0], _args[1], _args[2]),
        SYS_WRITE => write(_args[0], _args[1], _args[2]),
        SYS_REMOVE => remove(_args[0]),
        SYS_SEEK => seek(_args[0], _args[1]),
        SYS_TELL => tell(_args[0]),
        SYS_FSTAT => fstat(_args[0], _args[1]),
        _ => -1,
    }
}

fn halt() -> ! {
    kprintln!("Goodbye, World!");
    shutdown()
}

macro_rules! unwrap {
    ($x: expr) => {
        match $x {
            Some(inner) => inner,
            None => return -1,
        }
    };
}

const O_RDONLY: usize = 0x000;
const O_WRONLY: usize = 0x001;
const O_RDWR: usize = 0x002;
const O_CREATE: usize = 0x200;
const O_TRUNC: usize = 0x400;

const STDIN: usize = 0;
const STDOUT: usize = 1;
const STDERR: usize = 2;

macro_rules! has {
    ($flag: expr, $x: expr) => {
        ($flag & $x == $x)
    };
}

fn open(ptr: usize, flag: usize) -> isize {
    let file_name = unwrap!(get_str(ptr));

    // file name is an empty string
    if file_name.is_empty() {
        return -1;
    }

    // Cannot set O_WRONLY and O_RDWR at the same time
    if has!(flag, O_WRONLY) && has!(flag, O_RDWR) {
        return -1;
    }

    let id: Path = file_name.as_str().into();
    let exist = DISKFS.get().root_dir.lock().exists(&id);

    let result = {
        if has!(flag, O_TRUNC) || (!exist && has!(flag, O_CREATE)) {
            DISKFS.get().create(id)
        } else if exist {
            DISKFS.get().open(id)
        } else {
            Err(OsError::NoSuchFile)
        }
    };

    let file = unwrap!(result.ok());

    let current = current();
    let mut descriptors = current.descriptors.lock();
    let id = descriptors
        .last_key_value()
        .map(|(k, _)| *k + 1)
        .unwrap_or(STDERR + 1);

    descriptors.insert(id, (file, flag));

    id as isize
}

fn close(fd: usize) -> isize {
    if fd <= 2 {
        return 0;
    }

    match current().descriptors.lock().remove(&fd) {
        Some((file, _)) => {
            kprintln!("closing...");
            DISKFS.get().close(file);
            0
        }
        None => -1,
    }
}

fn read(fd: usize, buffer: usize, size: usize) -> isize {
    if size == 0 {
        return 0;
    }

    // validate buffer pointer
    unwrap!(Pointer::<u8>::from(buffer).check());
    unwrap!(Pointer::<u8>::from(buffer + size - 1).check());

    let mut ptr = buffer;

    unsafe {
        // read from stdin?
        if fd == 0 {
            for _ in 0..size {
                *(ptr as *mut u8) = console_getchar() as u8;
                ptr += 1;
            }
            size as isize
        } else {
            let current = current();
            let mut descriptor = current.descriptors.lock();

            let result = descriptor.get_mut(&fd).and_then(|(file, flag)| {
                if has!(*flag, O_WRONLY) {
                    None
                } else {
                    file.read(from_raw_parts_mut(ptr as *mut u8, size)).ok()
                }
            });

            unwrap!(result) as isize
        }
    }
}

fn write(fd: usize, buffer: usize, size: usize) -> isize {
    unwrap!(Pointer::<u8>::from(buffer).check());
    unwrap!(Pointer::<u8>::from(buffer + size - 1).check());

    if fd == STDOUT || fd == STDERR {
        let mut str = unwrap!(get_str(buffer));
        str.truncate(size);
        kprint!("{}", str);
        str.len() as isize
    } else {
        let current = current();
        let mut descriptor = current.descriptors.lock();

        let result = descriptor.get_mut(&fd).and_then(|(file, flag)| {
            if has!(*flag, O_WRONLY) || has!(*flag, O_RDWR) {
                unsafe { file.write(from_raw_parts(buffer as *mut u8, size)).ok() }
            } else {
                None
            }
        });

        unwrap!(result) as isize
    }
}

fn remove(ptr: usize) -> isize {
    let file_name = unwrap!(get_str(ptr));
    unwrap!(DISKFS.get().remove(file_name.as_str().into()).ok());
    0
}

fn seek(fd: usize, position: usize) -> isize {
    let current = current();
    let mut descriptor = current.descriptors.lock();

    let result = descriptor
        .get_mut(&fd)
        .and_then(|(file, _)| file.seek(SeekFrom::Start(position)).ok());

    unwrap!(result) as isize
}

fn tell(fd: usize) -> isize {
    let current = current();
    let mut descriptor = current.descriptors.lock();

    let result = descriptor
        .get_mut(&fd)
        .and_then(|(file, _)| file.stream_position().ok());

    unwrap!(result) as isize
}

fn fstat(fd: usize, ptr: usize) -> isize {
    unwrap!(Pointer::<usize>::from(ptr).check());
    unwrap!(Pointer::<usize>::from(ptr + 8usize).check());

    let current = current();
    let mut descriptor = current.descriptors.lock();
    let (file, _) = unwrap!(descriptor.get_mut(&fd));

    unsafe {
        *(ptr as *mut usize) = file.ino();
        *((ptr + 8) as *mut usize) = unwrap!(file.len().ok());
    }

    0
}

fn raw_execute_handler(_args: [usize; 3]) -> isize {
    let file_name = unwrap!(get_str(_args[0]));
    let mut ptr = _args[1];
    let mut argv = Vec::new();

    loop {
        let argv_ptr = unwrap!(Pointer::<usize>::from(ptr).take());

        if argv_ptr == 0 {
            break;
        }

        argv.push(unwrap!(get_str(argv_ptr)));
        ptr += 8;
    }

    kprintln!("prog to execute: {}.", file_name);
    let result = DISKFS.get().open(file_name.as_str().into());

    match result {
        Ok(file) => execute(file, argv),
        Err(err) => {
            kprintln!("file system err: {:?}", err);
            -1
        }
    }
}

fn get_str(mut ptr: usize) -> Option<String> {
    let mut str: Vec<char> = Vec::new();
    loop {
        let lastchar = Pointer::<u8>::from(ptr).take()?;
        if lastchar == 0u8 {
            break;
        }
        str.push(lastchar as char);
        ptr += 1
    }
    Some(str.iter().collect())
}

#[derive(Clone, Copy)]
pub struct Pointer<T>(*mut T);

impl<T> From<usize> for Pointer<T> {
    fn from(value: usize) -> Self {
        Pointer(value as *mut T)
    }
}

impl<T> Pointer<T> {
    pub fn check(&self) -> Option<*mut T> {
        let current = current();
        let pt = current.pagetable.as_ref().unwrap().lock();
        let entry = pt.get_pte(self.0 as usize);

        entry.and_then(|e| match e.is_user() && e.is_valid() {
            true => Some(self.0),
            false => None,
        })
    }
}

impl<T: Clone> Pointer<T> {
    pub fn take(&self) -> Option<T> {
        self.check().and_then(|ptr| unsafe { Some((*ptr).clone()) })
    }
}
