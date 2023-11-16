use super::*;

type String = heapless::String<64>;

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

