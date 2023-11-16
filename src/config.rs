use super::*;

use alloc::collections::BTreeMap as Map;
use serde::Deserialize;

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
pub struct LectureEntry {
    pub lecture: String,
}

#[derive(Debug)]
pub struct Config {
    pub qna: Vec<QnaEntry>,
    pub lecture: Option<LectureEntry>,
}

pub fn default_config() -> Config {
    let qna = vec![QnaEntry {
        name: "smth_part".into(),
        question: "smth".into(),
        led: 0,
        button: 0,
        correct_answer: "c_an".into(),
        wrong_answer: "c_wr".into(),
    }];
    println!("{} {}", qna[0].question, qna[0].name);

    Config { qna, lecture: None }
}

pub fn default_recordings() -> Vec<(String, &'static [u8])> {
    vec![("smth".into(), include_bytes!("../test.mp3").as_slice())]
}
