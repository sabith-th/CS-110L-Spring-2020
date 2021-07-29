use linked_list::ComputeNorm;
use linked_list::LinkedList;
pub mod linked_list;

#[derive(Debug, Clone)]
struct MyStruct {
    value: String,
}

fn main() {
    let mut list: LinkedList<String> = LinkedList::new();
    assert!(list.is_empty());
    assert_eq!(list.get_size(), 0);
    for i in 1..12 {
        list.push_front(format!("Node {}", i));
    }
    println!("{}", list);
    println!("list size: {}", list.get_size());
    println!("top element: {}", list.pop_front().unwrap());
    println!("{}", list);
    println!("size: {}", list.get_size());
    println!("{}", list.to_string()); // ToString impl for anything impl Display

    let mut cloneable_list: LinkedList<u32> = LinkedList::new();
    for i in 1..12 {
        cloneable_list.push_front(i);
    }
    println!("{}", cloneable_list);
    let mut clone_1 = cloneable_list.clone();
    clone_1.pop_front();
    println!("Original: {} Clone: {}", cloneable_list, clone_1);
    println!("Equal? : {}", clone_1 == cloneable_list);
    clone_1.push_front(11);
    println!("Original: {} Clone: {}", cloneable_list, clone_1);
    println!("Equal? : {}", clone_1 == cloneable_list);
    let mut clone_2 = list.clone();
    clone_2.pop_front();
    println!("Original: {} Clone: {}", list, clone_2);
    println!("Equal?: {}", list == clone_2);

    let mut my_struct_list: LinkedList<MyStruct> = LinkedList::new();
    my_struct_list.push_front(MyStruct {
        value: String::from("Shirjana"),
    });
    println!(
        "Original: {:?} Clone: {:?}",
        my_struct_list,
        my_struct_list.clone()
    );

    my_struct_list.push_front(MyStruct {
        value: String::from("Niroula"),
    });
    for val in my_struct_list {
        print!("{} ", val.value);
    }

    // If you implement iterator trait:
    for val in &list {
        println!("{}", val);
    }
    println!("{}", list);
    let uppercase_list: Vec<String> = list.into_iter().map(|f| f.to_uppercase()).collect();
    println!("{:?}", uppercase_list);

    let mut f64_list: LinkedList<f64> = LinkedList::new();
    for i in 1..11 {
        f64_list.push_front(i as f64);
    }
    println!("Norm: {}", f64_list.compute_norm());
}
