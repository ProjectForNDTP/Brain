use super::*;

use embedded_hal::{adc::OneShot, digital::v2::InputPin};

use core::convert::Infallible;

// static buttons: &mut Option<Vec<Box<dyn InputPin<Error = Infallible> + Sync + Send>>> = make_static!(None);

// /// Must be called only once
// pub fn init_buttons(pins: Vec<Box<dyn InputPin<Error = Infallible>>>) {
//     buttons.replace(Some(pins));
// }

pub async fn check_pressed(pins: &Vec<Box<dyn InputPin<Error = Infallible>>>) -> Option<usize> {
    println!("check Buttons");
    return Some(0);
    use core::ops::Deref;
    for (i, b) in pins.iter().enumerate() {
        println!("{i}");
        let pressed = InputPin::is_low(b.deref()).unwrap() && {
            Ticker::every(Duration::from_millis(50)).next().await;
            InputPin::is_low(b.deref()).unwrap()
        };
        if pressed {
            return Some(i);
        }
    }
    None
}
