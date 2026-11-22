fn powmod(base: u64, mut exponent: u64, modulus: u64) -> u64 {
    let (mut result, mut b) = (1 % modulus, base % modulus);
    while exponent > 0 {
        if exponent % 2 == 1 {
            result = result * b % modulus;
        }
        b = b * b % modulus;
        exponent /= 2;
    }
    result
}

fn main() {
    println!("{}", powmod(2, 10, 1000));
    println!("{}", powmod(3, 200, 40009));
    println!("{}", powmod(7, 123456, 30011));
    println!("{}", powmod(10, 0, 7));
}
