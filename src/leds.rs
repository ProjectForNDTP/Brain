use super::*;


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