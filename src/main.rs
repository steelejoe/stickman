#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::ram;
use esp_hal::rng::Rng;
use esp_hal::system::Stack;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::Config;
use esp_println::println;
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;
use stickman::app::{App, AppPins};
use stickman::net::{self, Core1Args};

// Required by recent espflash / ESP-IDF bootloaders.
esp_bootloader_esp_idf::esp_app_desc!();

/// Internal RAM for radio DMA / `malloc_internal`. PSRAM is added after `init`.
fn init_internal_heap() {
    // Reclaimed RAM + a dedicated internal region so Wi-Fi does not sit in PSRAM.
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 72 * 1024);
}

/// APP_CPU stack — Wi-Fi/HTTP/USB need more than 8 KiB.
const CORE1_STACK: usize = 32 * 1024;

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let _ = spawner;
    init_internal_heap();
    println!("Stickman Tamagotchi starting...");

    let config = Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // 8MB OPI PSRAM (ESP32-S3R8). Must be release builds.
    esp_alloc::psram_allocator!(&peripherals.PSRAM, esp_hal::psram);
    println!("Heap: PSRAM enabled (octal)");

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0);

    let rng = Rng::new();
    let seed = ((rng.random() as u64) << 32) | rng.random() as u64;

    let app_pins = AppPins {
        gpio0: peripherals.GPIO0,
        gpio2: peripherals.GPIO2,
        gpio3: peripherals.GPIO3,
        gpio5: peripherals.GPIO5,
        gpio6: peripherals.GPIO6,
        gpio7: peripherals.GPIO7,
        gpio17: peripherals.GPIO17,
        gpio18: peripherals.GPIO18,
        gpio38: peripherals.GPIO38,
        gpio47: peripherals.GPIO47,
        gpio48: peripherals.GPIO48,
        spi2: peripherals.SPI2,
        i2c0: peripherals.I2C0,
    };

    println!("WiFi: init radio on core 0");
    let radio = match esp_radio::init() {
        Ok(c) => c,
        Err(e) => {
            println!("WiFi: radio init failed: {e:?}");
            let mut app = App::new(app_pins);
            app.run().await;
        }
    };
    static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
    let radio = RADIO.init(radio);

    let (controller, interfaces) = match esp_radio::wifi::new(radio, peripherals.WIFI, Default::default())
    {
        Ok(v) => v,
        Err(e) => {
            println!("WiFi: controller failed: {e:?}");
            let mut app = App::new(app_pins);
            app.run().await;
        }
    };

    let args = Core1Args {
        controller,
        interfaces,
        usb0: peripherals.USB0,
        usb_dp: peripherals.GPIO20,
        usb_dm: peripherals.GPIO19,
        flash: peripherals.FLASH,
        seed,
    };

    static STACK: StaticCell<Stack<CORE1_STACK>> = StaticCell::new();
    println!("Starting core 1 (USB + HTTP + embassy-net)");
    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw.software_interrupt0,
        sw.software_interrupt1,
        STACK.init(Stack::new()),
        move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            EXECUTOR.init(Executor::new()).run(move |spawner| {
                spawner.spawn(core1_main(args)).ok();
            });
        },
    );

    let mut app = App::new(app_pins);
    app.run().await;
}

#[embassy_executor::task]
async fn core1_main(args: Core1Args) {
    let Core1Args {
        controller,
        interfaces,
        usb0,
        usb_dp,
        usb_dm,
        flash,
        seed,
    } = args;

    let (storage, creds) = net::creds::load(flash);
    static FLASH: StaticCell<esp_storage::FlashStorage<'static>> = StaticCell::new();
    let storage = FLASH.init(storage);

    let spawner = unsafe { embassy_executor::Spawner::for_current_executor().await };
    spawner
        .spawn(net::wifi::run(controller, interfaces, seed, creds))
        .ok();
    net::usb::run(usb0, usb_dp, usb_dm, storage).await;
}
