mod controller;
mod parser;
mod renderer;

use std::io;

fn main() -> io::Result<()> {
    controller::run()
}
