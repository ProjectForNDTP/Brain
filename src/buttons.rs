use super::*;

use embedded_hal::{adc::OneShot, digital::v2::InputPin};

use core::convert::Infallible;

// static buttons: &mut Option<Vec<Box<dyn InputPin<Error = Infallible> + Sync + Send>>> = make_static!(None);

// /// Must be called only once
// pub fn init_buttons(pins: Vec<Box<dyn InputPin<Error = Infallible>>>) {
//     buttons.replace(Some(pins));
// }

pub async fn check_pressed(pins: &[AnyPin<Input<PullUp>>]) -> Option<usize> {
    // println!("check Buttons");
    
    // return Some(0);
    for (i, b) in pins.iter().enumerate() {
        // println!("{i}");
        let pressed = b.is_low().unwrap() && {
            // println!("{i}");
            embassy_time::Timer::after_millis(50).await;
            b.is_low().unwrap()
        };
        if pressed {
            return Some(i);
        }
    }
    None
}
