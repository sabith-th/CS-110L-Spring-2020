use std::{thread, time};

struct ChannelMessage<T: Send + 'static> {
    index: usize,
    item: T,
}

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());
    output_vec.resize_with(input_vec.len(), Default::default);
    let (input_sender, input_receiver) = crossbeam_channel::unbounded::<ChannelMessage<T>>();
    let (result_sender, result_receiver) = crossbeam_channel::unbounded::<ChannelMessage<U>>();
    let mut threads = Vec::new();
    for _ in 0..num_threads {
        let input_receiver = input_receiver.clone();
        let result_sender = result_sender.clone();
        threads.push(thread::spawn(move || {
            while let Ok(input) = input_receiver.recv() {
                result_sender
                    .send(ChannelMessage {
                        index: input.index,
                        item: f(input.item),
                    })
                    .expect("Tried sending result to channel, but failed");
            }
            drop(result_sender);
        }));
    }
    drop(result_sender);
    let mut i = input_vec.len();
    while let Some(input) = input_vec.pop() {
        i -= 1;
        input_sender
            .send(ChannelMessage {
                index: i,
                item: input,
            })
            .expect("Tried sending input to channel, but failed");
    }
    drop(input_sender);
    while let Ok(result) = result_receiver.recv() {
        output_vec[result.index] = result.item;
    }
    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 16, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
