const N: i64 = 10_000_000;

struct Node {
    value: i64,
    next: Option<Box<Node>>,
}

fn main() {
    let mut head: Option<Box<Node>> = None;
    for i in 0..N {
        head = Some(Box::new(Node { value: i, next: head }));
    }
    let mut sum: i64 = 0;
    let mut cur = head.as_deref();
    while let Some(n) = cur {
        sum += n.value;
        cur = n.next.as_deref();
    }
    println!("{sum}");
}
