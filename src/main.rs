//! This example shows how to use the DAC on PIN 25 and 26
//! You can connect an LED (with a suitable resistor) or check the changing
//! voltage using a voltmeter on those pins.

//#![feature(restricted_std)]
#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![cfg(not(target_arch = "x86_64"))]

use core::{
    cell::{Cell, RefCell},
    ops::BitOr, str::FromStr,
};

use esp_backtrace as _;
use esp_println::println;

use esp32_hal::{
    adc::{AdcConfig, Attenuation, ADC, ADC2},
    clock::ClockControl,
    cpu_control::{CpuControl, Stack},
    dac::{self, DAC, DAC1, DAC2},
    embassy::{self, executor::Executor},
    gpio::{IO, AnyPin, Input, PullUp},
    interrupt::{self, Priority},
    ledc,
    peripherals::{self, Peripherals, TIMG0, TIMG1},
    prelude::*,
    prelude::*,
    spi::{master::Spi, SpiMode},
    timer::{Timer, Timer0, Timer1, TimerGroup},
    Delay,

};

use embassy_executor::{SendSpawner, Spawner};
use embassy_sync::{
    blocking_mutex::{
        self,
        raw::{CriticalSectionRawMutex, NoopRawMutex},
    },
    channel::Channel,
    mutex::Mutex,
    signal::Signal,
};
use embassy_time::{Duration, Instant, Ticker};

use log::{debug, error, info, warn};

use static_cell::make_static;

use alloc::{boxed::Box, rc::Rc, string::String, vec, vec::Vec};
use embedded_hal::adc::OneShot;

use smart_leds::{colors, SmartLedsWrite, White, RGB, RGBW};

// Allocator
extern crate alloc;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

const ALLOCATOR_MEM_SIZE: usize = 80_000;
// #[no_mangle]
// #[link_section = ".noinit"]
// static ALLOCATOR_MEM: core::mem::MaybeUninit<[u8; ALLOCATOR_MEM_SIZE]> =
//     core::mem::MaybeUninit::uninit();


// Audio packeting

mod audio;
mod buttons;
mod config;

type MulticoreMutex<T: Send> = Mutex<CriticalSectionRawMutex, T>;
type SinglecoreMutex<T: Send> = Mutex<NoopRawMutex, T>;

//static main_core_spawner: MulticoreMutex<Option<RefCell<SendSpawner>>> = None;
// static worker_core_spawner: MulticoreMutex<Option<RefCell<SendSpawner>>> = None;
static _core_spawner: Option<SendSpawner> = None;
static worker_core_spawner: Option<SendSpawner> = None;

// static decoder_interrupt: blocking_mutex::Mutex<CriticalSectionRawMutex, Cell<bool>> = blocking_mutex::Mutex::new(Cell::new(false));

#[main]
async fn main(_spawner: Spawner) -> ! {
    let mut smth = [0u8; 100_000];
    
    // main_core_spawner = Some(_spawner.make_send());
    //main_core_spawner.lock().await.replace(Some(_spawner.make_send()));

    // critical_section::with(|cs| {
    //     main_core_spawner.borrow(cs).as_mut().replace(_spawner.clone())
    // });

    // Init logger
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    println!("Here we go again!");

    // Init allocator
    // unsafe {
    //     ALLOCATOR.init(
    //         (&mut ALLOCATOR_MEM.assume_init()[0]) as *mut u8,
    //         ALLOCATOR_MEM_SIZE,
    //     )
    // };
    unsafe {
        ALLOCATOR.init(
            smth.as_mut_ptr(),
            smth.len(),
        )
    };

    let mut v = Vec::<i32>::new();
    for i in 0..10 {
        v.push(i);
    }
    // {
    //     let mut buf = Vec::new();
    //     const len: usize = 32_000;
    //     for i in 0..len {
    //         buf.push((i % 256) as u8);
    //     }
    //     for i in 0..len {
    //         if buf[i] != (i % 256) as u8 {
    //             println!("{i}: expected {}, got {}", buf[i], i % 255);
    //         }
    //     }
    // }

    // Init peripherals
    let peripherals = Peripherals::take();
    let system = peripherals.SYSTEM.split();
    let analog = peripherals.SENS.split();

    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    // let mut rtc = esp32_hal::Rtc::new(peripherals.RTC_CNTL);
    let mut delay = Delay::new(&clocks);

    // Init embassy
    let timer_group0 = esp32_hal::timer::TimerGroup::new(peripherals.TIMG0, &clocks);
    esp32_hal::embassy::init(&clocks, timer_group0.timer0);

    // Init IO pins
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
    
    use esp32_hal::gpio::InputPin;
    //let pin = io.pins.gpio16.enable_input(true).internal_pull_up(true);
    // let pin = io.pins.gpio16.into_pull_up_input();
    // let pin: esp32_hal::gpio::AnyPin<esp32_hal::gpio::Input<esp32_hal::gpio::PullUp>> = pin.degrade();
    
    let btn_pins = [
            io.pins.gpio35.into_pull_up_input().degrade(),
            io.pins.gpio17.into_pull_up_input().degrade(),
            io.pins.gpio18.into_pull_up_input().degrade(),
        ];
    let check_pressed = || buttons::check_pressed(&btn_pins[..]);

    // for i in 0..10000000 {
    //     println!("{:?}", pins[0].is_low());
    //     embassy_time::Timer::after_millis(500).await;
    // }

    // let check_pressed = || async {
    //     for (i, b) in pins.iter().enumerate() {
    //         println!("{i}");
    
    //         let pressed = b.is_low().unwrap() && {
    //             embassy_time::Timer::after_millis(50).await;
    //             b.is_low().unwrap()
    //         };
    //         if pressed {
    //             return Some(i);
    //         }
    //     }
    //     None
    // };

    // Prepare audio pipeline
    // let audio_timer = timer_group0.timer1;
    // let audio_timer_freq = 40_000_000u32;

    delay.delay(1u32);

    let mut dac = DAC2::dac(analog.dac1, io.pins.gpio26.into_analog()).unwrap();

    // Init worker
    let mut cpu_control = CpuControl::new(system.cpu_control);

    let worker_fnctn = move || {
        let executor = make_static!(Executor::new());
        executor.run(|spawner| {
            let writer = move |sample: u8| dac.write(sample);
            spawner.spawn(audio::worker(Box::new(writer))).ok();
            
            // worker_core_spawner.try_lock().unwrap().replace(Some(spawner.make_send()));
            // worker_core_spawner = Some(spawner.make_send());
        });
    };

    let WORKER_CORE_STACK: &mut Stack<8192> = make_static!(Stack::new());

    let _guard = cpu_control
        .start_app_core(WORKER_CORE_STACK, worker_fnctn)
        .unwrap();
    

    // ADC
    // let adc_reader = {
    //     use esp32_hal::adc::{AdcConfig, Attenuation, ADC, ADC2};
    //     let mut adc2_config = AdcConfig::new();
    //     let mut pin27 =
    //         adc2_config.enable_pin(io.pins.gpio27.into_analog(), Attenuation::Attenuation0dB);
    //     let mut adc2 = ADC::<ADC2>::adc(analog.adc2, adc2_config).unwrap();

    //     let mut delay = Delay::new(&clocks);

    //     // loop {
    //     //     let pin27_value: u16 = adc2.read(&mut pin27).unwrap();
    //     //     println!("PIN27 ADC reading = {}", pin27_value);
    //     //     delay.delay_ms(1500u32);
    //     // }
    //     move || adc2.read(&mut pin27).unwrap()
    // };

    // Light led
    // let sclk = io.pins.gpio19;
    // let miso = io.pins.gpio25;
    // let mosi = io.pins.gpio23;
    // let cs = io.pins.gpio22;


    // let spi = Spi::new(
    //     peripherals.SPI2,
    //     sclk,
    //     mosi,
    //     miso,
    //     cs,
    //     3u32.MHz(),
    //     SpiMode::Mode0,
    //     &clocks,
    // );
    // let mut led = ws2812_spi::Ws2812::new(spi);
    const LED_LEN: usize = 10;

    let mut buf = [RGB::default(); LED_LEN];

    // loop {
    //     use smart_leds::SmartLedsWrite;
    //     let t = Instant::now();
    //     SmartLedsWrite::write(&mut led, (0..50).map(|i| smart_leds::RGB::new(i, 0, 0))).unwrap();
    //     let t1 = t.elapsed().as_micros();
    //     println!("{}", t1);
    //     d.delay_ms(500u32);
    // }

    let mut led_writer = move |color: RGB<u8>, led_num: usize| {
        println!("Write led {color} {led_num}");
        buf[led_num] = color;
        // SmartLedsWrite::write(&mut led, (0..buf.len()).map(|i| buf[i])).unwrap();
    };
    // let led_writer = Rc::new(led_writer);

    // #[embassy_executor::task]
    // async fn checker() {
    //     loop {
    //         println!("free {}", ALLOCATOR.free());
    //         println!("used {}", ALLOCATOR.used());
    //         embassy_time::Timer::after_millis(50).await;
    //     }
    // }
    // _spawner.spawn(checker());
    // let mut l: Vec<i32> = Vec::new();
    // for i in 0..10 {
    //     l.push(i);
    // }
    // embassy_time::Timer::after_millis(500).await;

    // loop {}

    type String = heapless::String<64>;

    let recordings =
        [("smth", include_bytes!("../test.mp3").as_slice())];

    let question = String::from_str("smth").unwrap();

    let qna = [config::QnaEntry {
        name: "smth_part".into(),
        question: question.clone(),
        led: 0,
        button: 0,
        correct_answer: "c_an".into(),
        wrong_answer: "c_wr".into(),
    }];


        let decoder_ended = make_static!(Signal::<NoopRawMutex, ()>::new());
        let decoder_interrupt = make_static!(Signal::<NoopRawMutex, ()>::new());

    // Qna
    loop {
        println!("Check 1");
        embassy_time::Timer::after_millis(400).await;
        // println!("RD");
        let button_num = match check_pressed().await {
            None => continue,
            Some(num) => num,
        };
        // println!("RD1");
        let qna = qna
            .iter()
            .find(|entry| entry.button == button_num)
            .expect("Button {button_num} pressed but is not defined in qna");
        let led_num = qna.led;
        led_writer(colors::RED, led_num);

        // println!("qq {} {}", question.len(), question);
        // println!("qq {} {}", qna.question.len(), qna.question);
        // println!("qq {} {}", recordings[0].0.len(), recordings[0].0);

        let mut slice = recordings
            .iter()
            .find(|q| {
                println!("{:?} {qna:?}", q.0);
                q.0 == qna.question
            })
            .map(|q| q.1)
            .unwrap();
        let reader = move |out: &mut [u8]| {
            if out.len() == 0 {
                panic!("Reader output buffer if of length 0");
            }
            let len = out.len().min(slice.len());
            // println!("slice len {} {}", slice.len(), len);
            for i in 0..len {
                out[i] = slice[i];
            }
            slice = &slice[len..];
            if len == 0 {
                return None;
            }
            // println!("out");
            Some(len)
        };

        decoder_ended.reset();
        decoder_interrupt.reset();


        _spawner
            .spawn(audio::decoder(decoder_interrupt, decoder_ended, Box::new(reader)))
            .unwrap();

        let start_instant = Instant::now();
        while !decoder_ended.signaled() && !decoder_interrupt.signaled() {
            embassy_time::Timer::after_millis(500).await;
            // Ticker::every(Duration::from_millis(500)).next().await;
            // println!("RD2");
            if let Some(_button_num) = check_pressed().await {
                if _button_num != button_num || {
                    _button_num == button_num && start_instant.elapsed() > Duration::from_secs(4)
                } {
                    decoder_interrupt.signal(());
                }
            }
            // println!("RD3");
        }
        decoder_ended.wait().await;

        led_writer(colors::BLACK, led_num);
    }

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
