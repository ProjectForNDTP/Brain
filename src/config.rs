use super::*;

// type String = heapless::String<64>;
#[derive(Debug, PartialEq)]
pub enum Mode {
    Interactive,
    Qna,
}

#[derive(Debug)]
pub struct QnaEntry {
    pub name: String,
    pub question: String,
    pub led: usize,
    pub button: usize,
    pub correct_answer: String,
    pub wrong_answer: String,
}

#[derive(Debug)]
pub struct InteractiveEntry {
    pub name: String,
    pub interactive: Vec<String>,
    pub led: usize,
    pub button: usize,
}

#[derive(Debug)]
pub struct Config {
    pub qna: Vec<QnaEntry>,
    pub interactive: Vec<InteractiveEntry>,
}

pub fn mode_recording(mode: &Mode) -> &'static str {
    match mode {
        Mode::Interactive => "activate_interactive.mp3",
        Mode::Qna => "activate_qna.mp3",
    }
}

pub fn default_config() -> Config {
    // TODO: remap buttons and leds
    let qna = vec![
        QnaEntry {
            name: "1".into(),
            question: "question_1.mp3".into(),
            led: 0,
            button: 0,
            correct_answer: "correct_answer_for_1.mp3".into(),
            wrong_answer: "wrong_answer_1.mp3".into(),
        },
        QnaEntry {
            name: "2".into(),
            question: "question_2.mp3".into(),
            led: 1,
            button: 1,
            correct_answer: "correct_answer_for_2.mp3".into(),
            wrong_answer: "wrong_answer_2.mp3".into(),
        },
        QnaEntry {
            name: "3".into(),
            question: "question_3.mp3".into(),
            led: 2,
            button: 2,
            correct_answer: "correct_answer_for_3.mp3".into(),
            wrong_answer: "wrong_answer_3.mp3".into(),
        },
    ];

    let interactive = vec![
        InteractiveEntry {
            name: "cerebrum".into(),
            interactive: vec!["cerebrum.mp3".into()],
            led: 0,    // TODO
            button: 0, // TODO
        },
        InteractiveEntry {
            name: "medulla".into(),
            interactive: vec!["medulla.mp3".into()],
            led: 1,    // TODO
            button: 1, // TODO
        },
        InteractiveEntry {
            name: "hindbrain".into(),
            interactive: vec!["hindbrain_1.mp3".into(), "hindbrain_2.mp3".into()],
            led: 2,
            button: 2,
        },
    ];

    Config { qna, interactive }
}

// macro_rules! load_recordings {
//     (($load:expr)+) => {
//         vec![
//             ("smth".into(), include_bytes!("../test.mp3").as_slice())
//         ]
//     };
// }

macro_rules! load_recordings {
    ($basedir:expr, [ $($name:expr),+ ]) => {
        vec![
            $(
                ($name.into(), include_bytes!(concat!($basedir, $name)).as_slice()),
            )*
        ]
    }
}

pub fn default_recordings() -> Vec<(String, &'static [u8])> {
    let vec = load_recordings!(
        "../recordings/",
        [
            "activate_interactive.mp3",
            "activate_qna.mp3",
            "question_1.mp3",
            "correct_answer_for_1.mp3",
            "wrong_answer_1.mp3",
            "question_2.mp3",
            "correct_answer_for_2.mp3",
            "wrong_answer_2.mp3",
            "question_3.mp3",
            "correct_answer_for_3.mp3",
            "wrong_answer_3.mp3"
        ]
    );
    // check
    {
        let vec_has = |x| vec.iter().find(|r| &r.0 == x).is_some();
        let config = default_config();
        // assert!(config.interactive.is_none() || vec_has(&config.interactive.as_ref().unwrap().lecture));
        for i in config.interactive.iter() {
            assert!(i.interactive.iter().all(|i| vec_has(i)));
        }
        for i in config.qna.iter() {
            assert!(vec_has(&i.question));
            assert!(vec_has(&i.correct_answer));
            assert!(vec_has(&i.wrong_answer));
        }
    };

    // vec![("smth".into(), include_bytes!("../test.mp3").as_slice())]
    // vec![("smth".into(), include_bytes!("../test.mp3").as_slice())]
    vec
}
