use std::process;

fn main() {
    eprintln!("=== P1 Smoke Test: redox-driver-sys → linux-kpi → firmware-loader ===");
    eprintln!();

    let mut passed = 0;
    let mut failed = 0;

    // Test 1: redox-driver-sys pci module compiles and types are correct
    {
        let _vendor = redox_driver_sys::pci::PCI_VENDOR_ID_AMD;
        let _class = redox_driver_sys::pci::PCI_CLASS_DISPLAY;
        let loc = redox_driver_sys::pci::PciLocation {
            segment: 0,
            bus: 0x10,
            device: 0,
            function: 0,
        };
        let path = loc.scheme_path();
        assert!(path.contains("0010"), "scheme_path should contain bus");
        eprintln!("[PASS] redox-driver-sys::pci types and constants");
        passed += 1;
    }

    // Test 2: memory module types and constants
    {
        let ct = redox_driver_sys::memory::CacheType::DeviceMemory;
        assert_eq!(ct.suffix(), "dev");
        let ct = redox_driver_sys::memory::CacheType::WriteCombining;
        assert_eq!(ct.suffix(), "wc");
        let prot = redox_driver_sys::memory::MmioProt::READ_WRITE;
        assert!(prot.contains(redox_driver_sys::memory::MmioProt::READ));
        eprintln!("[PASS] redox-driver-sys::memory types and constants");
        passed += 1;
    }

    // Test 3: DMA buffer allocation
    {
        match redox_driver_sys::dma::DmaBuffer::allocate(4096, 64) {
            Ok(buf) => {
                assert!(!buf.as_ptr().is_null());
                assert_eq!(buf.len(), 4096);
                eprintln!(
                    "[PASS] redox-driver-sys::dma DmaBuffer allocation (virt={:#x}, phys={:#x})",
                    buf.as_ptr() as usize,
                    buf.physical_address()
                );
                passed += 1;
            }
            Err(e) => {
                eprintln!(
                    "[SKIP] redox-driver-sys::dma DmaBuffer (no /scheme/memory/translation): {}",
                    e
                );
            }
        }
    }

    // Test 4: IRQ handle types
    {
        // Just verify the types compile
        let _ = |irq: u32| -> redox_driver_sys::Result<redox_driver_sys::irq::IrqHandle> {
            redox_driver_sys::irq::IrqHandle::request(irq)
        };
        eprintln!("[PASS] redox-driver-sys::irq types compile");
        passed += 1;
    }

    // Test 5: linux-kpi memory allocation
    {
        let p = unsafe { linux_kpi::memory::kmalloc(64, 0) };
        assert!(!p.is_null(), "kmalloc should succeed");
        unsafe { linux_kpi::memory::kfree(p) };

        let p2 = unsafe { linux_kpi::memory::kzalloc(128, 0) };
        assert!(!p2.is_null(), "kzalloc should succeed");
        for i in 0..128 {
            assert_eq!(unsafe { *p2.add(i) }, 0, "kzalloc should zero memory");
        }
        unsafe { linux_kpi::memory::kfree(p2) };
        unsafe { linux_kpi::memory::kfree(std::ptr::null()) };
        eprintln!("[PASS] linux-kpi::memory kmalloc/kzalloc/kfree");
        passed += 1;
    }

    // Test 6: linux-kpi sync primitives
    {
        let mut mutex_mem: [u8; 64] = [0; 64];
        let mutex =
            unsafe { &mut *(&mut mutex_mem as *mut [u8; 64] as *mut linux_kpi::sync::LinuxMutex) };
        unsafe { linux_kpi::sync::mutex_init(mutex) };
        unsafe { linux_kpi::sync::mutex_lock(mutex) };
        unsafe { linux_kpi::sync::mutex_unlock(mutex) };

        let mut spinlock = linux_kpi::sync::Spinlock::default();
        unsafe { linux_kpi::sync::spin_lock_init(&mut spinlock) };
        unsafe { linux_kpi::sync::spin_lock(&mut spinlock) };
        unsafe { linux_kpi::sync::spin_unlock(&mut spinlock) };
        eprintln!("[PASS] linux-kpi::sync mutex and spinlock");
        passed += 1;
    }

    // Test 7: linux-kpi firmware struct
    {
        let fw = linux_kpi::firmware::Firmware::default();
        assert!(fw.data.is_null());
        assert_eq!(fw.size, 0);
        eprintln!("[PASS] linux-kpi::firmware Firmware struct");
        passed += 1;
    }

    // Test 8: linux-kpi DMA mapping API (no-op on Linux host)
    {
        let mut dma_handle: u64 = 0;
        let ptr = unsafe {
            linux_kpi::dma::dma_alloc_coherent(std::ptr::null_mut(), 4096, &mut dma_handle, 0)
        };
        if !ptr.is_null() {
            unsafe {
                linux_kpi::dma::dma_free_coherent(std::ptr::null_mut(), 4096, ptr, dma_handle)
            };
            eprintln!("[PASS] linux-kpi::dma dma_alloc/free_coherent");
            passed += 1;
        } else {
            eprintln!("[SKIP] linux-kpi::dma (requires /scheme/memory/translation)");
        }

        assert_eq!(
            unsafe { linux_kpi::dma::dma_set_mask(std::ptr::null_mut(), 0xFFFF_FFFF_FFFF_FFFF) },
            0
        );
        eprintln!("[PASS] linux-kpi::dma dma_set_mask");
        passed += 1;
    }

    // Test 9: linux-kpi io accessors (heap-backed, no real MMIO)
    {
        let ptr = unsafe { linux_kpi::io::ioremap(0x1000, 4096) };
        if !ptr.is_null() {
            unsafe { linux_kpi::io::writel(0xDEADBEEF, ptr) };
            let val = unsafe { linux_kpi::io::readl(ptr) };
            assert_eq!(val, 0xDEADBEEF, "readl should return writel value");
            unsafe { linux_kpi::io::writeq(0x12345678_9ABCDEF0u64, ptr) };
            let val64 = unsafe { linux_kpi::io::readq(ptr) };
            assert_eq!(val64, 0x12345678_9ABCDEF0u64);
            unsafe { linux_kpi::io::iounmap(ptr, 4096) };
            eprintln!("[PASS] linux-kpi::io readl/writel/readq/writeq");
            passed += 1;
        } else {
            eprintln!("[FAIL] linux-kpi::io ioremap returned null");
            failed += 1;
        }
    }

    // Test 10: linux-kpi PCI types
    {
        let mut dev = linux_kpi::pci::PciDev::default();
        dev.vendor = redox_driver_sys::pci::PCI_VENDOR_ID_AMD;
        dev.device = 0x7480;
        let result = unsafe { linux_kpi::pci::pci_enable_device(&mut dev) };
        assert_eq!(result, 0);
        assert!(dev.enabled);
        eprintln!("[PASS] linux-kpi::pci pci_enable_device");
        passed += 1;
    }

    eprintln!();
    eprintln!("=== Results: {} passed, {} failed ===", passed, failed);

    if failed > 0 {
        process::exit(1);
    }
}
