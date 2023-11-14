use super::*;

use embedded_sdmmc::{File, VolumeManager, Error, SdCard, TimeSource, Timestamp};
use core::cell::RefCell;
use embassy_sync::{mutex::Mutex, blocking_mutex::raw::{NoopRawMutex, CriticalSectionRawMutex}, signal::Signal, channel::Channel};
use rmp3::{Decoder, Frame, RawDecoder, MAX_SAMPLES_PER_FRAME};
use esp32_hal::{dac::DAC, timer::Instance};
use core::error::Error;
use heapless::Vec;
use alloc::boxed::Box;
use static_cell::make_static;

const READ_BUF: usize = 2048;

// struct MockTimestamp();

// impl TimeSource for MockTimestamp {
//     fn get_timestamp(&self) -> Timestamp {
//         Timestamp { year_since_1970: 0, zero_indexed_month: 0, zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
//     }
// }

// static volume: Option<Mutex<NoopRawMutex, RefCell<VolumeManager<SdCard, MockTimestamp>>>> = None;

const PacketQueueSize: usize = 4;
const PacketSampleslen: usize = 8_000;

struct AudioPacket {
    samples: Box<Vec<u8, PacketSampleslen>>,
    sample_rate: u32,
}

static AudioChannel: Channel<CriticalSectionRawMutex, AudioPacket, PacketQueueSize> = Channel::new();
static AudioChannelRecycle: Channel<CriticalSectionRawMutex, AudioPacket, PacketQueueSize> = Channel::new();

#[embassy_executor::task]
pub async fn decoder(interrupt: Signal<NoopRawMutex, bool>, reader: FnMut(&mut [u8]) -> Option<usize>) {
    // Initialize packet queue
    let queueInitialized = make_static!(false);
    if queueInitialized {
        for _ in 0..PacketQueueSize {
            AudioChannelRecycle.send(AudioPacket {
                sample_rate: 0,
                samples: Box::new(Vec::<u8, PacketSampleslen>::new()),
            }).await;
        }
        *queueInitialized = true;
    }

    let buf = [0u8; READ_BUF].as_mut_slice();
    let mut buf_remained = 0;
    let out_buf = [0u16; MAX_SAMPLES_PER_FRAME].as_mut_slice();

    // let mut packet = AudioChannelRecycle.receive().await;
    let mut packet = AudioPacket{
        samples: Box::new(Vec::new()),
        sample_rate: 0,  
    };

    packet.samples.clear();

    loop {
        // Sd card reader
        // let len = match volume.lock().await.read(file, &mut buf[buf_remained..]) {
        //     Ok(len) => len,
        //     Err(err) => {
        //         error!("Failed to read file contents {err:?}");
        //         return;
        //     },
        // };
        let len = match reader(&mut buf[buf_remained..]) {
            Some(len) => len,
            None => return,
        };

        let buf = &buf[..len];

        let t = Instant::now();
        let mut bfs_cnt = 0;
        let mut smp_cnt = 0;
        let mut decoder = RawDecoder::new();
                
        if let Some((frame, bytes_consumed)) = decoder.next(buf, out_buf) {
            buf_remained = buf.len() - bytes_consumed;
            buf[0..buf_remained] = buf[bytes_consumed..];

            if let Frame::Audio(frame) = frame {
                //println!("P {bfs_cnt}");
                //println!("{} {} {} {}", frame.bitrate(), frame.sample_count(), frame.sample_rate(), frame.channels());
                let samples = frame.samples();
                let channels = frame.channels() as usize;
                let sample_count = frame.sample_count();

                packet.sample_rate = frame.sample_rate();

                for i in 0..sample_count {
                    //packet.samples.push((samples[i * channels]  / 256 + 127) as u8);
                    packet.samples.push((samples[i * channels] as i16 / 256 + 128) as u8);
                    smp_cnt += 1;
                    if packet.samples.is_full() {
                        bfs_cnt += 1;
                        //AudioChannel.send(packet).await;
                        //packet = AudioChannelRecycle.receive().await;
                        packet.samples.clear();
                    }
                }
            }
        }

        if interrupt.signaled() {
            return;
        }
        println!("buffs {bfs_cnt}, samples {smp_cnt}");
        println!("END OF FILE, micros: {}", t.elapsed().as_micros());
    }
}

/// Shall be ran on fully dedicated core (does not yield)
#[embassy_executor::task]
pub async fn worker(dac: impl DAC, timer: impl Instance, ticks_per_second: u32) {
    let mut audio_packet = AudioChannel.receive().await;

    let mut slice = &audio_packet.samples[..];
    let mut ticks_per_sample = (ticks_per_second / audio_packet.sample_rate) as u64;
    let mut last = timer.now();
    
    loop {
        if slice.len() == 0 {
            // TODO: Add benchmarking
            AudioChannelRecycle.send(audio_packet).await;
            audio_packet = AudioChannel.receive().await;

            slice = &audio_packet.samples[..];
            ticks_per_sample = (ticks_per_second / audio_packet.sample_rate) as u64;

            if slice.len() == 0 {
                return;
            }
        }

        let now = timer.now();
        let dt = match now.checked_sub(last) {
            Some(dt) => dt,
            None => (u64::MAX - last) + now,
        };

        if dt >= ticks_per_sample {
            let sample = slice[0];
            
            dac.write(sample);

            let skip = (dt / ticks_per_sample) as usize;
            last = now - (dt % ticks_per_sample);

            if dt > ticks_per_sample + 50000 {
                println!("TOO LONG!!! : {dt} : {ticks_per_sample} :");
            }

            if slice.len() > skip {
                slice = &slice[skip..];
            } else {
                slice = &[];
            }
        }
    }
}