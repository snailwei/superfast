use superfast::{FastDecoder, FastEncoder};

#[test]
fn encoder_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<FastEncoder>();
}

#[test]
fn decoder_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<FastDecoder>();
}

#[test]
fn encoder_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<FastEncoder>();
}

#[test]
fn decoder_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<FastDecoder>();
}