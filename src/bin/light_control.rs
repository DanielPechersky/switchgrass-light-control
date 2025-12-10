#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::{error, info};
use esp_hal::{Async, timer::timg::TimerGroup};
use esp_hal::{
    clock::CpuClock,
    uart::{self, UartRx},
};

use esp_println as _;

use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    watch::{self, DynAnonReceiver, DynSender, Watch},
};
use embassy_time::Timer;

use esp_backtrace as _;

use switchgrass_light_control::{
    input::{LedValues, read_message},
    ws281x,
};

use smart_leds::{RGB8, SmartLedsWriteAsync};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const STRIP_LENGTH: usize = 50;
const WS281X_BYTES: usize = STRIP_LENGTH * 12;

type Ws281x = ws281x::Ws281x<'static, WS281X_BYTES>;

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Starting light control");

    static LED_VALUES: static_cell::ConstStaticCell<watch::Watch<NoopRawMutex, LedValues, 0>> =
        static_cell::ConstStaticCell::new(Watch::new());
    let led_values = LED_VALUES.take();

    let listener = UartRx::new(
        peripherals.UART0,
        uart::Config::default().with_baudrate(230400),
    )
    .unwrap()
    .with_rx(peripherals.GPIO3)
    .into_async();
    spawner.must_spawn(serial_listener_task(listener, led_values.dyn_sender()));

    let ws281x: Ws281x =
        ws281x::init::<WS281X_BYTES>(peripherals.SPI2, peripherals.GPIO13, peripherals.DMA_SPI2);
    spawner.must_spawn(lights_task(ws281x, led_values.dyn_anon_receiver()));
}

#[embassy_executor::task]
async fn serial_listener_task(
    mut uart: UartRx<'static, Async>,
    led_values: DynSender<'static, LedValues>,
) {
    loop {
        match read_message(&mut uart).await {
            Ok(leds) => led_values.send(leds),
            Err(e) => error!("Failed to read message from UART: {}", e),
        }
    }
}

#[embassy_executor::task]
async fn lights_task(mut ws281x: Ws281x, mut led_values: DynAnonReceiver<'static, LedValues>) {
    loop {
        let leds: LedValues = led_values.try_get().unwrap_or([u8::MAX; 150]);
        let (leds, remainder) = leds.as_chunks();
        assert!(remainder.is_empty());

        let leds = leds.iter().map(|&[r, g, b]| RGB8::new(r, g, b));

        ws281x.write(leds).await.unwrap();

        Timer::after_millis(1000 / 60).await;
    }
}
