//! Runs a Chasm script inside a Rust application.
//!
//! The host owns a wallet balance and lends scripts two capabilities:
//! `withdraw` and `log`. Each call's failure comes back as a value, and the
//! runtime keeps working afterwards.
//!
//! Run with `cargo run --example embed` from `host/`.

use std::cell::Cell;
use std::rc::Rc;

use chasm::{Runtime, Value};

const SCRIPT: &str = r#"
fn checkout(price: Int, quantity: Int) -> Int {
    let total = price * quantity
    let left = withdraw(total)
    log("paid")
    print(left)
    return left
}

fn spin() {
    while true {
    }
}
"#;

fn main() {
    let balance = Rc::new(Cell::new(100));
    let mut runtime = Runtime::new();

    let wallet = Rc::clone(&balance);
    runtime
        .register("withdraw", move |amount: i32| -> Result<i32, String> {
            if amount < 0 {
                return Err(format!("cannot withdraw a negative amount ({amount})"));
            }
            if amount > wallet.get() {
                return Err(format!(
                    "cannot withdraw {amount}; the balance is {}",
                    wallet.get()
                ));
            }
            wallet.set(wallet.get() - amount);
            Ok(wallet.get())
        })
        .expect("`withdraw` is a valid, unused name");
    runtime
        .register("log", |message: String| -> Result<(), String> {
            println!("  host log: {message}");
            Ok(())
        })
        .expect("`log` is a valid, unused name");

    let script = match runtime.compile(SCRIPT) {
        Ok(script) => script,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    runtime.set_fuel(10_000);

    let calls: [(&str, Vec<Value>); 5] = [
        ("checkout", vec![30.into(), 2.into()]),
        ("checkout", vec![50.into(), 1.into()]),
        ("checkout", vec![i32::MAX.into(), 2.into()]),
        ("spin", vec![]),
        ("checkout", vec![10.into(), 1.into()]),
    ];
    for (name, args) in calls {
        println!("{name}({args:?})");
        match runtime.call(&script, name, &args) {
            Ok(value) => println!("  returned {value:?}"),
            Err(error) => println!("  failed: {error}"),
        }
        let printed = runtime.take_output();
        if !printed.is_empty() {
            print!("  script printed: {printed}");
        }
        println!("  balance: {}", balance.get());
    }
}
