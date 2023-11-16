use super::*;

use alloc::boxed::Box;
use core::cell::RefCell;
use embedded_sdmmc::{Error, File, SdCard, TimeSource, Timestamp, VolumeManager};
use esp32_hal::{dac::DAC, timer::Instance};
use heapless::Vec;
use rmp3::{Decoder, Frame, RawDecoder, MAX_SAMPLES_PER_FRAME};
use static_cell::make_static;

const READ_BUF: usize = 512;

// struct MockTimestamp();

// impl TimeSource for MockTimestamp {
//     fn get_timestamp(&self) -> Timestamp {
//         Timestamp { year_since_1970: 0, zero_indexed_month: 0, zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
//     }
// }

// static volume: Option<Mutex<NoopRawMutex, RefCell<VolumeManager<SdCard, MockTimestamp>>>> = None;

const PacketQueueSize: usize = 5;
const PacketSampleslen: usize = 8_000;

struct AudioPacket {
    samples: Box<Vec<u8, PacketSampleslen>>,
    sample_rate: u32,
}

static AudioChannel: Channel<CriticalSectionRawMutex, AudioPacket, PacketQueueSize> =
    Channel::new();
static AudioChannelRecycle: Channel<CriticalSectionRawMutex, AudioPacket, PacketQueueSize> =
    Channel::new();

static mut QueueInitialized: bool = false;

#[embassy_executor::task]
pub async fn decoder(
    interrupt: &'static Signal<NoopRawMutex, ()>,
    ended: &'static Signal<NoopRawMutex, ()>,
    mut reader: Box<dyn FnMut(&mut [u8]) -> Option<usize>>,
) {
    println!("Start audio");

    // Initialize packet queue
    if !unsafe { QueueInitialized } {
        for _ in 0..PacketQueueSize {
            AudioChannelRecycle
                .send(AudioPacket {
                    sample_rate: 0,
                    samples: Box::new(Vec::<u8, PacketSampleslen>::new()),
                })
                .await;
        }
        unsafe { QueueInitialized = true };
    }

    let mut buf = [0u8; READ_BUF];
    let mut buf_previously_consumed = buf.len();
    let mut out_buf = [0i16; MAX_SAMPLES_PER_FRAME];

    let mut packet = AudioChannelRecycle.receive().await;
    // let mut packet = AudioPacket {
    //     samples: Box::new(Vec::new()),
    //     sample_rate: 0,
    // };

    packet.samples.clear();

    let mut bfs_cnt = 0;
    let mut smp_cnt = 0;
    let t = Instant::now();

    let mut decoder = RawDecoder::new();

    async {
        loop {
            // Copy unconsumed bytes
            // println!("A");
            let unconsumed = buf.len() - buf_previously_consumed;
            for i in 0..unconsumed {
                buf[i] = buf[buf_previously_consumed + i];
            }
            // println!("C");
            // println!("copied: buf[..{unconsumed}] = buf[{buf_previously_consumed}..{})]", buf_previously_consumed + unconsumed);

            // Read buffer
            let len = match reader(&mut buf[unconsumed..]) {
                Some(len) => len,
                None => {/* println!("Reader returned None"); */ return},
            };

            // println!("D");
            let buf = &mut buf[..(unconsumed + len)];
            // println!("availible: buf[..{}]", buf.len());

            // println!("E");

            if let Some((frame, bytes_consumed)) = decoder.next(buf, &mut out_buf) {
                buf_previously_consumed = bytes_consumed;
                // println!("buf_previously_consumed {buf_previously_consumed}");

                if let Frame::Audio(frame) = frame {
                    // println!("frame {} samples", frame.sample_count());
                    //println!("P {bfs_cnt}");
                    //println!("{} {} {} {}", frame.bitrate(), frame.sample_count(), frame.sample_rate(), frame.channels());
                    let samples = frame.samples();
                    let channels = frame.channels() as usize;
                    let sample_count = frame.sample_count();

                    packet.sample_rate = frame.sample_rate();


                    for i in 0..sample_count {
                        packet.samples.push((samples[i * channels]  / 256 + 127) as u8).unwrap();
                        packet
                            .samples
                            .push((samples[i * channels] as i16 / 256 + 128) as u8)
                            .unwrap();

                        smp_cnt += 1;
                        if packet.samples.is_full() {
                            bfs_cnt += 1;
                            AudioChannel.send(packet).await;
                            packet = AudioChannelRecycle.receive().await;
                            packet.samples.clear();
                        }
                    }

                }
                // } else {
                //     match frame {
                //         Frame::Other(a) => println!("other: {:?}", a),
                //         _ => {},
                //     }
                // }
            }

            if interrupt.signaled() {
                AudioChannel.send(packet).await;
                return;
            }
        }
    }
        .await;

    println!("buffs {bfs_cnt}, samples {smp_cnt}");
    println!("END OF FILE, micros: {}", t.elapsed().as_micros());

    println!("Signal ended");
    ended.signal(());
}

type TimerType<'a> =
    esp32_hal::timer::Timer0<esp32_hal::timer::TimerGroup<'a, esp32_hal::peripherals::TIMG0>>;

/// Shall be ran on fully dedicated core (does not yield)
#[embassy_executor::task]
pub async fn worker(mut writer: Box<dyn FnMut(u8) -> ()>) {
    let mut audio_packet = AudioChannel.receive().await;

    let mut slice = &audio_packet.samples[..];
    let ticks_per_second = embassy_time::TICK_HZ as u32;
    let mut ticks_per_sample = (ticks_per_second / audio_packet.sample_rate) as u64;

    println!("Worker started");

    let mut last = Instant::now();

    loop {
        // println!("A0");

        if slice.len() == 0 {
            // TODO: Add benchmarking
    // println!("Worker get");
            AudioChannelRecycle.send(audio_packet).await;
            audio_packet = AudioChannel.receive().await;
            // println!("Worker got");

            slice = &audio_packet.samples[..];
            ticks_per_sample = (ticks_per_second / audio_packet.sample_rate) as u64;

            if slice.len() == 0 {
                return;
            }
        }
        // println!("A");

        let now = Instant::now();
        // println!("A1");
        // let dt = match now.checked_sub(last) {
        //     Some(dt) => dt,
        //     None => (u64::MAX - last) + now,
        // };
        let dt = now.duration_since(last).as_ticks();

        // println!("B");
        if dt >= ticks_per_sample {
            // println!("B1");
            let sample = slice[0];

            // dac.write(sample);

            // println!("C");
            writer(sample);
            // println!("D");

            let skip = (dt / ticks_per_sample) as usize;
            // println!("D1");
            last = now
                .checked_sub(Duration::from_ticks(dt % ticks_per_sample))
                .unwrap_or(now);
            // println!("E");

            if dt > ticks_per_sample + 10000 {
                println!("TOO LONG!!! : {dt} : {ticks_per_sample} :");
            }

            if slice.len() > skip {
                // println!("A1");
                slice = &slice[skip..];
                // println!("A2");
            } else {
                slice = &[];
            }
        }
        // println!("F");
    }
}
