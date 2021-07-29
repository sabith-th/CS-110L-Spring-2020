use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Too few arguments.");
        process::exit(1);
    }
    let mut line_count = 0;
    let mut word_count = 0;
    let mut char_count = 0;
    let filename = &args[1];
    let file = File::open(filename).expect("Unable to open file");
    for line in BufReader::new(file).lines() {
        line_count += 1;
        let line = line.unwrap();
        word_count += line.split_whitespace().count();
        char_count += line.chars().count();
    }
    char_count += line_count; // Include newline to character count
    println!(
        "Lines: {} Words: {} Chars: {}",
        line_count, word_count, char_count
    );
}
