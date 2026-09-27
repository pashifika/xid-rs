fn main() -> Result<(), xid::GenerationError> {
    println!("{}", xid::try_new()?);
    Ok(())
}
