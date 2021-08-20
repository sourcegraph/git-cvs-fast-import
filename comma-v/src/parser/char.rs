pub(super) fn is_printable_ascii(c: u8) -> bool {
    (0x20..0x7f).contains(&c)
}

pub(super) fn is_printable_ascii_without<const N: usize>(c: u8, exclude: &[u8; N]) -> bool {
    exclude
        .iter()
        .fold(is_printable_ascii(c), |acc, excluded| acc && c != *excluded)
}

pub(super) fn is_idchar(c: u8) -> bool {
    is_printable_ascii_without(c, b"$,.:;@")
}

pub(super) fn is_intchar(c: u8) -> bool {
    is_printable_ascii_without(c, b"@") || c == 0x0c
}
