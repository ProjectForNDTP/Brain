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
    future::Future,
    ops::BitOr,
    str::FromStr,
};

use esp_backtrace as _;
use esp_println::println;

use esp32_hal::{
    adc::{AdcConfig, Attenuation, ADC, ADC2},
    clock::ClockControl,
    cpu_control::{CpuControl, Stack},
    dac::{self, DAC, DAC1, DAC2},
    embassy::{self, executor::Executor},
    gpio::{AnyPin, Input, PullUp, PullDown, IO},
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
        Mutex,
    },
    channel::Channel,
    signal::Signal,
};
use embassy_time::{Duration, Instant, Ticker};

use log::{debug, error, info, warn};

use static_cell::make_static;

use alloc::{boxed::Box, collections::btree_set::Iter, rc::Rc, string::String, vec, vec::Vec};
use embedded_hal::adc::OneShot;

use smart_leds::{colors, SmartLedsWrite, White, RGB, RGBW};

// Allocator
extern crate alloc;

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

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
    // main_core_spawner = Some(_spawner.make_send());
    //main_core_spawner.lock().await.replace(Some(_spawner.make_send()));

    // critical_section::with(|cs| {
    //     main_core_spawner.borrow(cs).as_mut().replace(_spawner.clone())
    // });

    // Init logger
    esp_println::logger::init_logger(log::LevelFilter::Debug);

    println!("Here we go again!");

    let mut heap = [0u8; 100_000];
    unsafe { ALLOCATOR.init(heap.as_mut_ptr(), heap.len()) };

    // {
    //     use core::{mem::MaybeUninit, ptr::addr_of_mut};

    //     const HEAP_SIZE: usize = (48 + 96) * 1024;
    //     static mut HEAP: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

    //     unsafe {
    //         let heap_size = HEAP.len();
    //         info!("Heap size: {}", heap_size);
    //         ALLOCATOR.init(addr_of_mut!(HEAP).cast(), heap_size);
    //     }
    // }

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

    let mode_pin = io.pins.gpio16.into_pull_up_input().degrade();
    async fn check_mode(pin: &AnyPin<Input<PullUp>>) -> config::Mode {
        let first = pin.is_low().unwrap();
        embassy_time::Timer::after_millis(50).await;
        let second = pin.is_low().unwrap();

        let convert = |x| match x {
            true => config::Mode::Interactive,
            false => config::Mode::Qna,
        };

        if first == second {
            convert(first)
        } else {
            embassy_time::Timer::after_millis(50).await;
            convert(pin.is_low().unwrap())
        }
    }
    let check_mode = || check_mode(&mode_pin);

    let btn_pins = [
        io.pins.gpio17.into_pull_down_input().degrade(),
        io.pins.gpio18.into_pull_down_input().degrade(),
        // io.pins.gpio17.into_pull_up_input().degrade(),
        // io.pins.gpio18.into_pull_up_input().degrade(),
    ];

    async fn check_pressed(pins: &[AnyPin<Input<PullDown>>]) -> Option<usize> {
        for (i, b) in pins.iter().enumerate() {
            // println!("{i}");
            let is_pressed = || b.is_high().unwrap();
            let pressed = is_pressed() && {
                // println!("{i}");
                embassy_time::Timer::after_millis(50).await;
                is_pressed()
            };
            if pressed {
                return Some(i);
            }
        }
        None
    }
    let check_pressed = || check_pressed(&btn_pins[..]);

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
    use esp32_hal::adc::{AdcConfig, Attenuation, ADC, ADC2};
    let mut adc2_config = AdcConfig::new();
    let mut pin27 =
        adc2_config.enable_pin(io.pins.gpio27.into_analog(), Attenuation::Attenuation0dB);
    let mut adc2 = ADC::<ADC2>::adc(analog.adc2, adc2_config).unwrap();

    let adc_reader = {
        let mut delay = Delay::new(&clocks);

        // loop {
        //     let pin27_value: u16 = adc2.read(&mut pin27).unwrap();
        //     println!("PIN27 ADC reading = {}", pin27_value);
        //     delay.delay_ms(1500u32);
        // }
        move || (nb::block!(adc2.read(&mut pin27)).unwrap()) as u32
    };

    // Watch audio aplification level read from adc
    let audio_level = Rc::new(Mutex::<NoopRawMutex, Cell<Option<u32>>>::new(Cell::new(
        None,
    )));

    #[embassy_executor::task]
    async fn audio_level_watcher(
        mut reader: Box<dyn FnMut() -> u32>,
        reader_max: u32,
        audio_level: Rc<Mutex<NoopRawMutex, Cell<u32>>>,
    ) {
        let mut ticker = Ticker::every(Duration::from_millis(100));
        let mut last_level = reader();
        loop {
            let level = reader();
            if last_level != level {
                // println!("level {level}");
                audio_level.lock(|x| x.replace(level));
                last_level = level;
            }
            ticker.next().await;
        }
    }

    // Spawn audio level watcher
    // _spawner
    //     .spawn(audio_level_watcher(
    //         Box::new(adc_reader),
    //         4095,
    //         audio_level.clone(),
    //     ))
    //     .unwrap();

    // Light led
    // Addresable led strip
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
    // const LED_LEN: usize = 10;

    // let mut buf = [RGB::default(); LED_LEN];

    // let mut led_writer = move |color: RGB<u8>, led_num: usize| {
    //     println!("Write led {color} {led_num}");
    //     buf[led_num] = color;
    //     SmartLedsWrite::write(&mut led, (0..buf.len()).map(|i| buf[i])).unwrap();
    // };

    // Plain leds
    let mut led_pins = [
        io.pins.gpio2.into_push_pull_output().degrade(),
        // io.pins.gpio3.into_push_pull_output().degrade(),
        // io.pins.gpio4.into_push_pull_output().degrade(),
    ];
    let mut buf: Vec<RGB<u8>> = (0..led_pins.len()).map(|_| RGB::default()).collect();

    let mut led_writer = move |color: RGB<u8>, led_num: usize| {
        println!("Write led {color} {led_num}");
        buf[led_num] = color;
        for (pin, color) in led_pins.iter_mut().zip(buf.iter()) {
            match color {
                &colors::BLACK => pin.set_low().unwrap(),
                _ => pin.set_high().unwrap(),
            };
        }
    };


    loop {
        let color = match check_pressed().await {
            Some(_) => colors::RED,
            None => colors::BLACK,
        };
        for i in 0..1 {
            led_writer(color, i);
        }
    }

    let await_pressed = |timeout: Duration| async move {
        let start = Instant::now();
        let mut ticker = Ticker::every(Duration::from_micros(50));
        while start.elapsed() < timeout {
            if let Some(n) = check_pressed().await {
                return Some(n);
            }
            ticker.next().await;
        }
        None
    };

    let await_release = || async move {
        let mut ticker = Ticker::every(Duration::from_micros(50));
        while let Some(_) = check_pressed().await {
            ticker.next().await;
        }
    };

    // reader
    let reader = |mut slice: &'static [u8]| {
        move |out: &mut [u8]| {
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
        }
    };

    let recordings = config::default_recordings();
    let _config = config::default_config();
    let qna = _config.qna;
    let interactive = _config.interactive; 

    let get_recording = |name: &str| {
        recordings
            .iter()
            .find(|q| q.0 == name)
            .map(|q| q.1)
            .unwrap()
    };

    'change_mode: loop {
        let last_mode = check_mode().await;

        // Play mode switch audio
        let recording = config::mode_recording(&last_mode);
        let _reader = reader(get_recording(recording));

        let decoder_ended = Rc::new(Signal::<NoopRawMutex, ()>::new());
        let decoder_interrupt = Rc::new(Signal::<NoopRawMutex, ()>::new());

        _spawner
            .spawn(audio::decoder(
                decoder_interrupt.clone(),
                decoder_ended.clone(),
                Box::new(_reader),
                audio_level.clone(),
                4095,
            ))
            .unwrap();

        // Await button release
        await_release().await;

        // Stop playing when any button is pressed
        // Uncomment to appy
        // while !decoder_ended.signaled() && !decoder_interrupt.signaled() {
        //     embassy_time::Timer::after_millis(50).await;
        //     if let Some(button_num) = check_pressed().await {
        //         decoder_interrupt.signal(());
        //         await_release().await;
        //     }
        // }
        decoder_ended.wait().await;

        match last_mode {
            config::Mode::Qna => {
                // Qna
                let looping_iter = {
                    let mut i = qna.iter();
                    let qna = &qna;
                    core::iter::from_fn(move || {
                        if let Some(s) = i.next() {
                            Some(s)
                        } else {
                            i = qna.iter();
                            i.next()
                        }
                    })
                };

                for qna in looping_iter {
                    // Play question recording
                    let decoder_ended = Rc::new(Signal::<NoopRawMutex, ()>::new());
                    let decoder_interrupt = Rc::new(Signal::<NoopRawMutex, ()>::new());

                    let _reader = reader(get_recording(qna.question.as_str()));

                    _spawner
                        .spawn(audio::decoder(
                            decoder_interrupt.clone(),
                            decoder_ended.clone(),
                            Box::new(_reader),
                            audio_level.clone(),
                            4095,
                        ))
                        .unwrap();

                    // Stop recording if some buttion is pressed (save pressed button)
                    let mut button_pressed = None;

                    while !decoder_ended.signaled() && !decoder_interrupt.signaled() {
                        embassy_time::Timer::after_millis(50).await;
                        if let Some(button_num) = check_pressed().await {
                            button_pressed = Some(button_num);
                            decoder_interrupt.signal(());
                            await_release().await;
                        }
                    }
                    decoder_ended.wait().await;

                    // Await action if no button was pressed
                    if let None = button_pressed {
                        button_pressed = await_pressed(Duration::from_secs(60 * 10)).await;
                    }

                    // Light led
                    let led_num = qna.led;
                    led_writer(colors::RED, led_num);

                    // Choose answer recording based on pressed button
                    let recording = match button_pressed {
                        Some(n) if n == qna.button => qna.correct_answer.as_str(),
                        _ => qna.wrong_answer.as_str(),
                    };

                    // Play answer recording
                    let _reader = reader(get_recording(recording));

                    let decoder_ended = Rc::new(Signal::<NoopRawMutex, ()>::new());
                    let decoder_interrupt = Rc::new(Signal::<NoopRawMutex, ()>::new());

                    _spawner
                        .spawn(audio::decoder(
                            decoder_interrupt.clone(),
                            decoder_ended.clone(),
                            Box::new(_reader),
                            audio_level.clone(),
                            4095,
                        ))
                        .unwrap();

                    // Await button release
                    await_release().await;

                    // Stop playing when any button is pressed again
                    while !decoder_ended.signaled() && !decoder_interrupt.signaled() {
                        embassy_time::Timer::after_millis(50).await;
                        if let Some(_button_num) = check_pressed().await {
                            decoder_interrupt.signal(());
                            await_release().await;
                        }
                    }
                    decoder_ended.wait().await;

                    led_writer(colors::BLACK, led_num);

                    if check_mode().await != last_mode {
                        continue 'change_mode;
                    };
                }
            }
            config::Mode::Interactive => {
                // Interactive
                loop {
                    embassy_time::Timer::after_millis(50).await;
                    // println!("RD");
                    let button_num = match check_pressed().await {
                        None => continue,
                        Some(num) => num,
                    };

                    println!("Button {button_num} pressed");

                    // println!("RD1");
                    let interactive = interactive
                        .iter()
                        .find(|entry| entry.button == button_num)
                        .expect("Button {button_num} pressed but is not defined in interactive");
                    let led_num = interactive.led;
                    led_writer(colors::RED, led_num);

                    // println!("qq {} {}", qna.question.len(), qna.question);
                    // println!("qq {} {}", recordings[0].0.len(), recordings[0].0);

                    let reader = reader(get_recording(interactive.interactive.as_str()));

                    let decoder_ended = Rc::new(Signal::<NoopRawMutex, ()>::new());
                    let decoder_interrupt = Rc::new(Signal::<NoopRawMutex, ()>::new());

                    // decoder_ended.reset();
                    // decoder_interrupt.reset();

                    _spawner
                        .spawn(audio::decoder(
                            decoder_interrupt.clone(),
                            decoder_ended.clone(),
                            Box::new(reader),
                            audio_level.clone(),
                            4095,
                        ))
                        .unwrap();

                    let start_instant = Instant::now();
                    while !decoder_ended.signaled() && !decoder_interrupt.signaled() {
                        embassy_time::Timer::after_millis(50).await;
                        // Ticker::every(Duration::from_millis(500)).next().await;
                        // println!("RD2");
                        if let Some(_button_num) = check_pressed().await {
                            if _button_num != button_num
                                || (_button_num == button_num
                                    && start_instant.elapsed() > Duration::from_secs(4))
                            {
                                decoder_interrupt.signal(());
                                await_release().await;
                            }
                        }
                        // println!("RD3");
                    }
                    decoder_ended.wait().await;

                    led_writer(colors::BLACK, led_num);

                    if check_mode().await != last_mode {
                        continue 'change_mode;
                    };
                }
            }
        }
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
