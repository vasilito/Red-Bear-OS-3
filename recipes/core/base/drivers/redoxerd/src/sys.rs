pub fn exit_success() {
    imp::exit(true);
}

pub fn exit_failure() {
    imp::exit(false);
}

pub fn debug_char(b: u8) {
    let _ = imp::write_debug(b);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod imp {
    use syscall::Io;
    use syscall::Pio;

    pub fn exit(success: bool) {
        if success {
            Pio::<u16>::new(0x604).write(0x2000);
            Pio::<u8>::new(0x501).write(51 / 2);
        } else {
            Pio::<u8>::new(0x501).write(53 / 2);
        }
    }

    pub fn write_debug(b: u8) -> syscall::Result<()> {
        Pio::<u8>::new(0xe9).write(b);
        Ok(())
    }
}

#[cfg(target_arch = "aarch64")]
mod imp {
    use qemu_exit::QEMUExit;

    pub fn exit(success: bool) {
        let q = qemu_exit::AArch64::new();
        if success {
            q.exit(51)
        } else {
            q.exit(53)
        }
    }

    pub fn write_debug(b: u8) -> syscall::Result<()> {
        // TODO
        Ok(())
    }
}

#[cfg(target_arch = "riscv64")]
mod imp {

    pub fn exit(success: bool) {
        todo!()
        // let q = qemu_exit::RISCV64::new(addr);
        // if success {
        //     q.exit(51)
        // } else {
        //     q.exit(53)
        // }
    }

    pub fn write_debug(b: u8) -> syscall::Result<()> {
        // TODO
        Ok(())
    }
}
