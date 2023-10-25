#![feature(type_alias_impl_trait)]
#![feature(restricted_std)]
#![feature(asm_experimental_arch)]

use log::{info};
use core::time::Duration;
use std::ffi::c_schar;
use std::ffi::CStr;
use std::ffi::c_void;

// use embassy_time::Timer;
// use embassy_executor::Executor;
// use embassy_executor::Spawner;
// use static_cell::StaticCell;

//pub use embassy_macros::{task, main_std};

// ~/espup ...
// source ~/esp....sh
// cd esp-test5
// cargo run

#[no_mangle]
extern "C" fn test(_: *mut core::ffi::c_void) {
        info!("Hello from `test`");
        unsafe {
        esp_idf_svc::hal::task::destroy(esp_idf_svc::hal::task::current().unwrap());
        }
        esp_idf_svc::hal::task::do_yield();
    }

fn main() {
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    
    let mut h: Vec<_> = (0..0).map(|n| {
        std::thread::spawn(move ||
            {
                let n = n;
                loop {
                    for i in 0..50000 {
                        let mut c = Vec::new();
                        c.push(1);
                        c.push(2);
                    }
                    info!("Hello from new thread! {n}, core {:?}", esp_idf_svc::hal::cpu::core());
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        )
    }).collect();
    // unsafe {
    //     esp_idf_svc::hal::task::create(test, CStr::from_bytes_with_nul_unchecked("test task\0".as_bytes()), 8000, unsafe {0 as *mut c_void}, 10, None)
    //     .unwrap();
    // }
    std::thread::spawn(move ||
        {
            //let state = false;
            let mut pin = esp_idf_svc::hal::gpio::PinDriver::output(unsafe { esp_idf_svc::hal::gpio::Gpio19::new() }).unwrap();
                
            loop {
                for _ in 0..500_000 {
                    pin.set_high();
                    pin.set_low();
                }

                std::thread::yield_now();

                //info!("Hello from new thread! 10");
                //std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    ).join();
    std::thread::sleep(std::time::Duration::from_secs(10));
    unreachable!();
}

// #[embassy_executor::task]
// async fn main1(s: Spawner) {
    
//     // It is necessary to call this function once. Otherwise some patches to the runtime
//     // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
//     esp_idf_svc::sys::link_patches();

//     // Bind the log crate to the ESP Logging facilities
//     esp_idf_svc::log::EspLogger::initialize_default();

//     info!("Hello, world!");
    
// #[embassy_executor::task(pool_size = 10)]
//     async fn task(s: i32) {
//         loop {
//         info!("Hello, world! {s}");

//         Timer::after_secs(1).await;
//         }
//     }

//     for i in 0..5 {
//         s.spawn(task(i)).unwrap();
//     }

//         loop {
//         info!("Hello, world! main");

//         Timer::after_secs(1).await;
//         }
// }
