use std::process;

use redox_driver_sys::dma::DmaBuffer;

fn run() -> Result<(), String> {
    println!("=== Red Bear OS DMA Runtime Check ===");

    let mut one_page =
        DmaBuffer::allocate(4096, 4096).map_err(|err| format!("alloc 4K failed: {err}"))?;
    println!(
        "dma_4k cpu={:#x} phys={:#x} len={:#x}",
        one_page.as_ptr() as usize,
        one_page.physical_address(),
        one_page.len()
    );
    unsafe {
        (one_page.as_mut_ptr() as *mut u32).write_volatile(0x1122_3344);
        let value = (one_page.as_ptr() as *const u32).read_volatile();
        println!("dma_4k_value={:#x}", value);
    }

    let mut two_page =
        DmaBuffer::allocate(8192, 4096).map_err(|err| format!("alloc 8K failed: {err}"))?;
    println!(
        "dma_8k cpu={:#x} phys={:#x} len={:#x}",
        two_page.as_ptr() as usize,
        two_page.physical_address(),
        two_page.len()
    );
    unsafe {
        let second_page = two_page.as_mut_ptr().add(4096) as *mut u32;
        second_page.write_volatile(0x5566_7788);
        let value = (two_page.as_ptr().add(4096) as *const u32).read_volatile();
        println!("dma_8k_second_page_value={:#x}", value);
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-phase-dma-check: {err}");
        process::exit(1);
    }
}
