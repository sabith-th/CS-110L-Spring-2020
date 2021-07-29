// Simple Hangman Program
// User gets five incorrect guesses
// Word chosen randomly from words.txt
// Inspiration from: https://doc.rust-lang.org/book/ch02-00-guessing-game-tutorial.html
// This assignment will introduce you to some fundamental syntax in Rust:
// - variable declaration
// - string manipulation
// - conditional statements
// - loops
// - vectors
// - files
// - user input
// We've tried to limit/hide Rust's quirks since we'll discuss those details
// more in depth in the coming lectures.
extern crate rand;
use rand::Rng;
use std::fmt::{Display, Formatter, Result};
use std::fs;
use std::io;
use std::io::Write;

const NUM_INCORRECT_GUESSES: u32 = 5;
const WORDS_PATH: &str = "words.txt";

fn pick_a_random_word() -> String {
    let file_string = fs::read_to_string(WORDS_PATH).expect("Unable to read file.");
    let words: Vec<&str> = file_string.split('\n').collect();
    String::from(words[rand::thread_rng().gen_range(0, words.len())].trim())
}

struct Chars(Vec<char>);

fn main() {
    let secret_word = pick_a_random_word();
    // Note: given what you know about Rust so far, it's easier to pull characters out of a
    // vector than it is to pull them out of a string. You can get the ith character of
    // secret_word by doing secret_word_chars[i].
    let secret_word_chars: Vec<char> = secret_word.chars().collect();
    // Uncomment for debugging:
    // println!("random word: {}", secret_word);

    // Your code here! :)
    let mut guessed_word: Chars = Chars(vec!['_'; secret_word.len()]);
    let mut guesses_left = NUM_INCORRECT_GUESSES;
    let mut guesses: Chars = Chars(Vec::new());
    while guesses_left > 0 {
        println!("The word so far is {}", guessed_word);
        println!("You have guessed the following letters: {}", guesses);
        println!("You have {} guesses left", guesses_left);
        println!("Please guess a letter");
        io::stdout().flush().expect("Error flushing stdout");
        let mut guess = String::new();
        io::stdin()
            .read_line(&mut guess)
            .expect("Error reading line");
        let char = guess.chars().next().expect("No letter found");
        guesses.0.push(char);
        let mut matched = false;
        let mut guessed = true;
        for (i, _c) in secret_word_chars.iter().enumerate() {
            if char == secret_word_chars[i] {
                guessed_word.0[i] = char;
                matched = true;
            }
            if guessed_word.0[i] == '_' {
                guessed = false;
            }
        }
        if guessed {
            println!(
                "Congratulations! You've guessed the secret word: {:?}",
                secret_word
            );
            break;
        }
        if !matched {
            guesses_left -= 1;
        }
    }

    if guesses_left == 0 {
        println!("Sorry, you ran out of guesses!");
    }
}

impl Display for Chars {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        for c in &self.0 {
            write!(f, "{}", c)?;
        }
        Ok(())
    }
}
