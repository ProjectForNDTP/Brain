use super::*;

use alloc::{string::String};
use alloc::collections::BTreeMap as Map;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct QnaEntry {
    question: String,
    led: usize,
    button: usize,
    correct_answer: String,
    wrong_answer: String,
}

#[derive(Debug, Deserialize)]
struct LectureEntry {
    lecture: String,
}

#[derive(Debug, Deserialize)]
struct Config {
    qna: Map<String, QnaEntry>,
    lecture: Option<LectureEntry>,
}

impl Default for Config {
    fn default() -> Self {
        let mut qna = Map::new();
        let entry = QnaEntry {
            question: "".into(),
            led: 0,
            button: 0,
            correct_answer: "".into(),
            wrong_answer: "".into(),
        };
        qna.insert("smth", entry);

        Self {
            qna: qna,
            lecture: None,
        }
    }
}

fn default_recordings() -> Map<String, &'static [u8]> {
    let mut map = Map::new();
    map.insert("key".into(), make_static!("../test.mp3"));

    map
}