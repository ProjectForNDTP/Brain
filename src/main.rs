//! This example shows how to use the DAC on PIN 25 and 26
//! You can connect an LED (with a suitable resistor) or check the changing
//! voltage using a voltmeter on those pins.

//#![feature(restricted_std)]
#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::{cell::RefCell, ops::BitOr};

use critical_section::Mutex;

use esp_backtrace as _;
use esp_println::println;

use esp32_hal::{
    cpu_control::{CpuControl, Stack},
    clock::ClockControl,
    interrupt,
    interrupt::Priority,
    peripherals::{self, Peripherals, TIMG0, TIMG1},
    prelude::*,
    timer::{Timer, Timer0, Timer1, TimerGroup},
    dac, gpio::IO, prelude::*, Delay,
    dac::DAC1,
    dac::DAC,
    psram,
    embassy::{self, executor::Executor},
    ledc,
};

use embassy_executor::{Spawner};
use embassy_sync::{blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex}, signal::Signal, channel::Channel};
use embassy_time::{Duration, Ticker, Instant};

use log::{error, warn, info, debug};

use static_cell::make_static;

// Allocator
extern crate alloc;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

const ALLOCATOR_MEM_SIZE: usize = 80_000;
static ALLOCATOR_MEM: core::mem::MaybeUninit<[u8; ALLOCATOR_MEM_SIZE]> = core::mem::MaybeUninit::uninit();

// Audio packeting

mod audio;
mod config;

type MulticoreMutex<T> = Mutex<CriticalSectionRawMutex, T>;
type SingleMutex<T> = Mutex<NoopRawMutex, T>;

static main_core_spawner: MulticoreMutex<RefCell<Option<Spawner>>> = None;
static worker_core_spawner: MulticoreMutex<RefCell<Option<Spawner>>> = None;

#[main]
async fn main(_spawner: Spawner) -> ! {
    critical_section::with(|cs| {
        main_core_spawner.borrow(cs).as_mut().replace(_spawner.clone())
    });

    // Init logger
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    // Init allocator
    unsafe { ALLOCATOR.init(&mut ALLOCATOR_MEM.assume_init()[0] as *mut u8, ALLOCATOR_MEM_SIZE) };

    // Init peripherals
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let analog = peripherals.SENS.split();

    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    // Init IO pins
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    // Init embassy
    let timer_group0 = esp32_hal::timer::TimerGroup::new(peripherals.TIMG0, &clocks);
    esp32_hal::embassy::init(&clocks, timer_group0.timer0);

    // 
    let mut rtc = esp32_hal::Rtc::new(peripherals.RTC_CNTL);
    let mut delay = Delay::new(&clocks);

    // Prepare audio pipeline
    let audio_timer = timer_group0.timer1;
    let audio_timer_freq = 40_000_000u32;

    let dac = DAC1::dac(analog.DAC, io.pins.gpio26.into_analog()).unwrap();

    // Init worker
    let mut cpu_control = CpuControl::new(system.cpu_control);
    
    let worker_fnctn = move || {
        let executor = make_static!(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(audio::worker(dac, audio_timer, audio_timer_freq)).ok();
            critical_section::with(|cs| {
                worker_core_spawner.borrow(cs).as_mut().replace(spawner)
            });
        });
    };

    let WORKER_CORE_STACK: &mut Stack<8192> = make_static!(Stack::new());

    let _guard = cpu_control
        .start_app_core(unsafe { WORKER_CORE_STACK }, worker_fnctn)
        .unwrap();

    //

    loop {}

    // ADC
    // use embedded_hal::adc::Channel;
    // {
    //     use esp32_hal::adc::{AdcConfig, Attenuation, ADC, ADC2};
    //     let mut adc2_config = AdcConfig::new();
    //     let mut pin27 =
    //         adc2_config.enable_pin(io.pins.gpio27.into_analog(), Attenuation::Attenuation0dB);
    //     let mut adc2 = ADC::<ADC2>::adc(analog.adc2, adc2_config).unwrap();
    
    //     let mut delay = Delay::new(&clocks);
    
    //     loop {
    //         let pin27_value: u16 = adc2.read(&mut pin27)..unwrap();
    //         println!("PIN27 ADC reading = {}", pin27_value);
    //         delay.delay_ms(1500u32);
    //     }
    // }

    // {
    //     let sclk = io.pins.gpio19;
    //     let miso = io.pins.gpio25;
    //     let mosi = io.pins.gpio23;
    //     let cs = io.pins.gpio22;
        
    //     use esp32_hal::spi::{master::Spi, SpiMode};
    //     let mut spi = Spi::new(peripherals.SPI2,
    //         sclk,
    //         mosi,
    //         miso,
    //         cs,
    //         3u32.MHz(), SpiMode::Mode0, &clocks
    //     );
    //     let mut led = ws2812_spi::Ws2812::new(spi);
    //     loop {
    //         use smart_leds::SmartLedsWrite;
    //         let t = Instant::now();
    //         SmartLedsWrite::write(&mut led, (0..50).map(|i| smart_leds::RGB::new(i, 0, 0))).unwrap();
    //         let t1 = t.elapsed().as_micros();
    //         println!("{}", t1);
    //         d.delay_ms(500u32);
    //     }
    // }
    // loop {}
/*
    let mut l1 = 0;
    let mut t = embassy_time::Instant::now();

    loop {
        let _l1 = timer10.now();
        let el = _l1 - l1;
        let t1 = t.elapsed();
        t = embassy_time::Instant::now();
        l1 = _l1;
        println!("{} {}", el, t1.as_micros());
        d.delay_ms(5u32);
    }

    timer10.start(2u64.secs());
    timer10.listen();

*/

    // PWM logic
    // use esp32_hal::ledc::Speed;
    // let ledc = esp32_hal::ledc::LEDC::new(peripherals.LEDC, &clocks);
    // let mut hstimer0 = ledc.get_timer::<ledc::HighSpeed>(ledc::timer::Number::Timer0);

    // let pwm_freq = 100_000u32;

    // hstimer0
    //     .configure(ledc::timer::config::Config {
    //         duty: ledc::timer::config::Duty::Duty8Bit,
    //         clock_source: ledc::timer::HSClockSource::APBClk,
    //         frequency: pwm_freq.Hz(),
    //     })
    //     .unwrap();


    // let mut channel0 = ledc.get_channel(ledc::channel::Number::Channel0, led);
    // channel0
    //     .configure(ledc::channel::config::Config {
    //         timer: &hstimer0,
    //         duty_pct: 10,
    //         pin_config: ledc::channel::config::PinConfig::PushPull,
    //     })
    //     .unwrap();

    // loop {
    //     // Set up a breathing LED: fade from off to on over a second, then
    //     // from on back off over the next second.  Then loop.
    //     channel0.start_duty_fade(0, 100, 1000).unwrap();
    //     while channel0.is_duty_fade_running() {}
    //     channel0.start_duty_fade(100, 0, 1000).unwrap();
    //     while channel0.is_duty_fade_running() {}
    // }
    
    // Old audio logic
    /*
    loop {
        dac1.write(data[cnt]);
        cnt += 1;
        if cnt >= DATA_LEN {
            cnt = 0; 
        } else {
            cnt = cnt;
        }
        d.delay_us(10u32);
    }
    */

    info!("PRE0");
    // critical_section::with(|cs| {
    //     let mut v = AudioChannelRecycle.borrow_ref_mut(cs);
    //     for i in 0..PacketQueueSize {
    //         info!("PRE");
    //         v.push(AudioPacket {
    //             sample_rate: 0,
    //             samples: alloc::boxed::Box::new(heapless::Vec::<u8, 1024>::new()),
    //         });
    //     }
    // });

    // for _ in 0..10 {
    //     let mut l11 = timer10.now();
    //     AudioChannel_1.send(AudioPacket {
    //         sample_rate: 0,
    //         samples: alloc::boxed::Box::new(heapless::Vec::<u8, 1024>::new()),
    //     }).await;
    //     let mut l21 = timer10.now();
    //     debug!("l11 l21 {}", l21 - l11);
    //     let mut l12 = timer10.now();
    //     AudioChannel_1.receive().await;
    //     let mut l22 = timer10.now();
    //     debug!("l12 l22 {}", l22 - l12);
    // }
    // for _ in 0..10 {
    //     let mut l1 = timer10.now();
    //     critical_section::with(|cs| AudioChannelRecycle.borrow_ref_mut(cs).pop());
    //     let mut l2 = timer10.now();
    //     debug!("l1 l2 {}", l2 - l1);
    // }
    // info!("AAA");
    // let mut audio_packet = {
    //     let mut packet: Option<AudioPacket> = None;
    //     while let None = packet {
    //         let packet = critical_section::with(|cs| AudioChannel.borrow_ref_mut(cs).pop());
    //     }
    //     packet.unwrap()
    // };
}

/*

#[interrupt]
fn TG1_T0_LEVEL() {
    critical_section::with(|cs| {
unsafe {
    DAC_PIN_1.borrow_ref_mut(cs).as_mut().unwrap().write(DATA[DATA_I]);
};
    unsafe { DATA_I += 1;
    if DATA_I == DATA_LEN {
        unsafe { DATA_I = 0 };
    }};

        let mut timer = TIMER10.borrow_ref_mut(cs);
        let timer = timer.as_mut().unwrap();

        if timer.is_interrupt_set() {
            timer.clear_interrupt();
            timer.start(1u64.micros());

            unsafe {
                CNT += 1;
                if CNT > 100_000 {
                    CNT = 0;
                    esp_println::println!("Interrupt Level 3 - Timer0");
                }
            }
        }
    });
}
*/