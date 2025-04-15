use rand::Rng;

pub fn random_string(lenght: usize) -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(lenght)
        .map(char::from)
        .collect()
}
