#![no_std]
#![no_main]

extern crate alloc;

use core::mem::MaybeUninit;
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    main,
    Config,
};
use esp_println::println;
use stickman::app::App;

// Required by recent espflash / ESP-IDF bootloaders.
esp_bootloader_esp_idf::esp_app_desc!();

fn init_heap() {
    const HEAP_SIZE: usize = 64 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP.as_mut_ptr() as *mut u8,
            HEAP_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}

#[main]
fn main() -> ! {
    init_heap();
    println!("Stickman Tamagotchi starting...");

    let config = Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let mut app = App::new(peripherals);
    app.run();
}
