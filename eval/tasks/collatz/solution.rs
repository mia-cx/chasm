fn steps(mut n: u64) -> u32 {
    let mut count = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        count += 1;
    }
    count
}

fn main() {
    for n in 1..=10 {
        println!("{}", steps(n));
    }
    let (mut best, mut best_steps) = (1, 0);
    for n in 1..10000 {
        let s = steps(n);
        if s > best_steps {
            best = n;
            best_steps = s;
        }
    }
    println!("{best}");
    println!("{best_steps}");
}
