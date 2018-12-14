use failure::{self, Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTree<'a> {
    pub pieces: Vec<Piece<'a>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Piece<'a> {
    pub title: Option<&'a [u8]>,
    pub composer: Option<&'a [u8]>,
    pub tempo: Option<u64>,
    pub beats: Option<u64>,

    pub voices: Vec<Voice<'a>>,
    pub plays: Vec<Play<'a>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Voice<'a> {
    pub name: Option<&'a [u8]>,
    pub program: Option<u8>,
    pub channel: Option<u8>,
    pub transpose: Option<i8>,
    pub volume: Option<u8>,
    pub drums: Option<bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Play<'a> {
    pub name: Option<&'a [u8]>,
    pub grand_staves: Vec<GrandStave<'a>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GrandStave<'a> {
    pub staves: Vec<Stave<'a>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stave<'a> {
    pub prefix: Option<&'a [u8]>,
    //     pub bars: Vec<BarTypeNode>,
}

fn is_whitespace(ch: u8) -> bool {
    match ch {
        b' ' | b'\t' | b'\r' => true,
        _ => false,
    }
}

struct Parser<'a> {
    pub source: &'a [u8],
    pub cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Parser {
            source: source.as_bytes(),
            cursor: 0,
        }
    }

    #[allow(dead_code)]
    fn debug_position(&self) {
        let before = self.cursor - std::cmp::min(self.cursor, 20);
        let end = std::cmp::min(self.cursor + 100, self.source.len());
        eprintln!(
            "{}«{}»",
            std::str::from_utf8(&self.source[before..self.cursor]).unwrap(),
            std::str::from_utf8(&self.source[self.cursor..end]).unwrap()
        );
    }

    #[allow(dead_code)]
    #[allow(unused_variables)]
    #[inline(always)]
    fn log(&self, message: &str) {
        #[cfg(feature = "verbose")]
        {
            eprint!("{}: ", message);
            self.debug_position();
        }
    }

    #[inline(always)]
    pub fn finished(&self) -> bool {
        self.cursor == self.source.len()
    }

    #[inline(always)]
    pub fn check(&self, next: &[u8]) -> bool {
        let end = self.cursor + next.len();
        end <= self.source.len() && &self.source[self.cursor..end] == next
    }

    pub fn skip(&mut self, next: &[u8]) -> bool {
        let skipped = self.check(next);
        if skipped {
            self.cursor += next.len();
            self.skip_whitespace();
        }
        skipped
    }

    pub fn skip_only(&mut self, next: &[u8]) -> bool {
        let skipped = self.check(next);
        if skipped {
            self.cursor += next.len();
        }
        skipped
    }

    pub fn expect(&mut self, next: &[u8]) -> Result<(), Error> {
        if self.finished() {
            return Err(failure::err_msg(format!(
                "Expected `{}` but reached the end of the file.",
                ::std::str::from_utf8(next).unwrap()
            )));
        }

        let next_byte = self.source[self.cursor];

        if !self.skip(next) {
            Err(failure::err_msg(format!(
                "Expected `{}` but saw `{}`",
                ::std::str::from_utf8(next).unwrap(),
                ::std::str::from_utf8(&[next_byte]).unwrap(),
            )))
        } else {
            Ok(())
        }
    }

    pub fn check_keyword(&mut self, keyword: &[u8]) -> bool {
        fn is_ident_char(ch: u8) -> bool {
            ch == b'_'
                || (b'a' <= ch && ch <= b'z')
                || (b'A' <= ch && ch <= b'Z')
                || (b'0' <= ch && ch <= b'9')
        }

        let end = self.cursor + keyword.len();
        self.check(keyword) && (end == self.source.len() || !is_ident_char(self.source[end]))
    }

    pub fn skip_keyword(&mut self, keyword: &[u8]) -> bool {
        let success = self.check_keyword(keyword);
        if success {
            self.cursor += keyword.len();
            self.skip_whitespace();
        }
        success
    }

    pub fn skip_whitespace(&mut self) {
        let mut in_comment = false;
        loop {
            if self.skip_only(b"//") {
                in_comment = true;
            } else if self.skip_only(b"\n") {
                in_comment = false;
            } else {
                if self.finished() || !(in_comment || is_whitespace(self.source[self.cursor])) {
                    break;
                }

                self.cursor += 1;
            }
        }
    }

    pub fn skip_whitespace_in_line(&mut self) {
        let mut in_comment = false;
        loop {
            if self.skip_only(b"//") {
                in_comment = true;
            } else {
                if self.finished()
                    || self.check(b"\n")
                    || !(in_comment || is_whitespace(self.source[self.cursor]))
                {
                    break;
                }

                self.cursor += 1;
            }
        }
    }

    pub fn check_attr(&mut self) -> Option<&'a [u8]> {
        fn is_attr_char(ch: u8) -> bool {
            ch == b'_'
                || ch == b','
                || ch == b'\''
                || ch == b'#'
                || (b'a' <= ch && ch <= b'z')
                || (b'A' <= ch && ch <= b'Z')
                || (b'0' <= ch && ch <= b'9')
        }

        let mut end = self.cursor;
        while end < self.source.len() {
            if is_attr_char(self.source[end]) {
                end += 1;
            } else {
                break;
            }
        }

        if end == self.cursor {
            None
        } else {
            Some(&self.source[self.cursor..end])
        }
    }

    pub fn parse_attr(&mut self) -> Option<&'a [u8]> {
        let attr = self.check_attr();
        if let Some(attr) = attr {
            self.cursor += attr.len();
            self.skip_whitespace();
        }
        attr
    }

    pub fn parse_number_only<T: std::str::FromStr>(&mut self) -> Result<T, Error> {
        fn is_digit(ch: u8) -> bool {
            ch >= b'0' && ch <= b'9'
        }

        let mut end = self.cursor;
        while end < self.source.len() {
            if is_digit(self.source[end]) || (end == self.cursor && self.source[end] == b'-') {
                end += 1;
            } else {
                break;
            }
        }

        let result: &str = std::str::from_utf8(&self.source[self.cursor..end])
            .map_err(|_| failure::err_msg("Invalid attribute value - must be utf8"))?;
        let result: T = result
            .parse()
            .map_err(|_| failure::err_msg("Could not parse number"))?;

        self.cursor = end;
        Ok(result)
    }

    pub fn parse_string_only(&mut self) -> Result<&'a [u8], Error> {
        // We only accept UTF-8 so this should be safe.
        let source_str = unsafe { std::str::from_utf8_unchecked(&self.source[self.cursor..]) };

        let mut started = false;
        let mut escaping = false;
        for (i, ch) in source_str.char_indices() {
            if started {
                match ch {
                    '\\' if !escaping => escaping = true,
                    '"' if !escaping => {
                        self.cursor += i + 1;
                        return Ok(&source_str[1..i].as_bytes());
                    }
                    _ => escaping = false,
                }
            } else {
                if ch != '"' {
                    return Err(failure::err_msg("String must open with `\"`"));
                }
                started = true;
            }
        }

        Err(failure::err_msg("Unclosed string!"))
    }

    pub fn parse_bool_only(&mut self) -> Result<bool, Error> {
        if self.skip_keyword(b"true") {
            Ok(true)
        } else if self.skip_keyword(b"false") {
            Ok(false)
        } else {
            Err(failure::err_msg("Failed to parse bool."))
        }
    }

    pub fn skip_end_of_stave(&mut self) -> bool {
        self.finished() || self.skip_only(b"\n") || self.skip_only(b";") || self.check(b"}")
    }

    pub fn skip_stave_contents(&mut self) {
        while !self.skip_end_of_stave() {
            self.cursor += 1;
        }
    }
}

pub fn parse<'a>(input: &'a str, _filename: Option<&'a str>) -> Result<ParseTree<'a>, Error> {
    let parser = &mut Parser::new(input);

    let mut pieces = Vec::new();

    parser.skip_whitespace();

    loop {
        parser.log("parse loop");

        pieces.push(parse_piece(parser)?);

        if parser.finished() {
            break;
        }
    }

    Ok(ParseTree { pieces })
}

fn parse_piece<'a>(parser: &mut Parser<'a>) -> Result<Piece<'a>, Error> {
    if parser.skip_keyword(b"piece") {
        parser.expect(b"{")?;
        let piece = parse_piece_contents(parser)?;
        parser.expect(b"}")?;
        Ok(piece)
    } else {
        let piece = parse_piece_contents(parser)?;
        Ok(piece)
    }
}

fn parse_piece_contents<'a>(parser: &mut Parser<'a>) -> Result<Piece<'a>, Error> {
    enum BlockType<'a> {
        Play(Option<&'a [u8]>),
        Voice(Option<&'a [u8]>),
    }

    let mut piece = Piece::default();

    loop {
        parser.log("parse_piece_contents loop");

        let block_type = {
            if parser.skip_keyword(b"play") {
                BlockType::Play(parser.parse_attr())
            } else if parser.skip_keyword(b"voice") {
                BlockType::Voice(parser.parse_attr())
            } else if let Some(attr_name) = parser.parse_attr() {
                parser.expect(b":")?;

                // TODO: more ugly duplication...
                match attr_name {
                    b"tempo" => piece.tempo = Some(parser.parse_number_only()?),
                    b"beats" => piece.beats = Some(parser.parse_number_only()?),
                    b"title" => piece.title = Some(parser.parse_string_only()?),
                    b"composer" => piece.composer = Some(parser.parse_string_only()?),
                    _ => return Err(failure::err_msg("Invalid attribute name")),
                }

                parser.skip_whitespace_in_line();
                let attribute_ended = parser.finished()
                    || parser.skip(b",")
                    || parser.skip(b"\n")
                    || parser.skip(b";")
                    || parser.check(b"}");

                if !attribute_ended {
                    return Err(failure::err_msg(
                        "Attributes must end with a newline, comma, or semi-colon.",
                    ));
                }

                continue;
            } else {
                parser.skip_whitespace();

                let done = parser.finished() || parser.check(b"}");
                if !done {
                    // Top-level contents are considered a play block
                    piece.plays.push(parse_play_contents(parser, None)?);
                    parser.skip_whitespace();
                }

                break;
            }
        };

        parser.expect(b"{")?;
        match block_type {
            BlockType::Play(name) => {
                piece.plays.push(parse_play_contents(parser, name)?);
            }
            BlockType::Voice(name) => {
                piece.voices.push(parse_voice_contents(parser, name)?);
            }
        }
        parser.expect(b"}")?;
    }

    Ok(piece)
}

fn parse_voice_contents<'a>(
    parser: &mut Parser<'a>,
    name: Option<&'a [u8]>,
) -> Result<Voice<'a>, Error> {
    let mut voice = Voice {
        name,
        ..Voice::default()
    };

    while let Some(attr_name) = parser.parse_attr() {
        parser.expect(b":")?;

        match attr_name {
            b"program" => voice.program = Some(parser.parse_number_only()?),
            b"channel" => voice.channel = Some(parser.parse_number_only()?),
            b"octave" => voice.transpose = Some(parser.parse_number_only::<i8>()? * 12),
            b"volume" => voice.volume = Some(parser.parse_number_only()?),
            b"drums" => voice.drums = Some(parser.parse_bool_only()?),
            _ => return Err(failure::err_msg("Invalid attribute name")),
        }

        parser.skip_whitespace_in_line();
        if !(parser.skip(b",") || parser.skip(b"\n") || parser.skip(b";")) {
            break;
        }
    }

    Ok(voice)
}

fn parse_play_contents<'a>(
    parser: &mut Parser<'a>,
    name: Option<&'a [u8]>,
) -> Result<Play<'a>, Error> {
    let mut play = Play {
        name,
        ..Play::default()
    };

    loop {
        parser.log("parse_play_contents loop");

        let attr_name = parser.parse_attr();

        if parser.skip(b":") {
            if parser.skip_only(b"|") {
                // Parse a stave
                play.grand_staves
                    .push(parse_grand_stave(parser, attr_name)?);
            } else {
                // Parse an attribute value
                return Err(failure::err_msg(
                    "Attributes in play blocks not currently supported. Use `|` to start a stave.",
                ));
            }
        } else {
            if let Some(attr_name) = attr_name {
                return Err(failure::err_msg(format!(
                    "Attribute `{}` is missing a value.",
                    std::str::from_utf8(attr_name).unwrap()
                )));
            }

            parser.skip_whitespace();
            break;
        }
    }
    Ok(play)
}

fn parse_grand_stave<'a>(
    parser: &mut Parser<'a>,
    first_stave_prefix: Option<&'a [u8]>,
) -> Result<GrandStave<'a>, Error> {
    let mut grand_stave = GrandStave::default();

    parser.log("Before the crime?");
    parser.skip_whitespace_in_line();
    parser.log("After the crime?");

    grand_stave
        .staves
        .push(parse_stave_contents(parser, first_stave_prefix)?);

    // More staves - TODO: kinda ugly duplication
    loop {
        parser.log("parse_grand_stave loop");
        if parser.skip_end_of_stave() {
            parser.skip_whitespace();
            break;
        }

        let attr_name = parser.parse_attr();

        if parser.skip(b":") {
            if parser.skip_only(b"|") {
                // Parse a stave
                parser.skip_whitespace_in_line();
                grand_stave
                    .staves
                    .push(parse_stave_contents(parser, attr_name)?);
            } else {
                // Parse an attribute value
                return Err(failure::err_msg("This is an issue huh, we can't set attributes from within this function. Kind of a pickle, oops."));
            }
        } else {
            if let Some(attr_name) = attr_name {
                return Err(failure::err_msg(format!(
                    "Attribute `{}` is missing a value.",
                    std::str::from_utf8(attr_name).unwrap()
                )));
            }
            break;
        }
    }

    Ok(grand_stave)
}

fn parse_stave_contents<'a>(
    parser: &mut Parser<'a>,
    stave_prefix: Option<&'a [u8]>,
) -> Result<Stave<'a>, Error> {
    loop {
        parser.log("parse_stave_contents loop");

        // TODO: Parse stave contents on the current line
        parser.skip_stave_contents();

        parser.skip_whitespace_in_line();
        if parser.skip_only(b"|") {
            // Continue the same stave
        } else {
            break;
        }
    }

    Ok(Stave {
        prefix: stave_prefix,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    // TODO: more tests covering parse failure

    use super::*;

    fn parse_succeeds(source: &str, result: ParseTree) {
        assert_eq!(parse(source, None).unwrap(), result);
    }

    fn parse_equivalent(variants: &[&str], result: ParseTree) {
        for variant in variants {
            parse_succeeds(variant, result.clone());
        }
    }

    fn parse_fails(source: &str) {
        assert!(parse(source, None).is_err());
    }

    fn plays_tree(plays: &[Play<'static>]) -> ParseTree<'static> {
        ParseTree {
            pieces: vec![Piece {
                plays: plays.to_owned(),
                ..Piece::default()
            }],
        }
    }

    #[test]
    fn parse_empty_piece() {
        parse_equivalent(
            &["", "  piece {}", "piece {}", "piece {\t   \n}"],
            ParseTree {
                pieces: vec![Piece::default()],
            },
        );
    }

    #[test]
    fn parse_empty_pieces() {
        parse_equivalent(
            &["piece{}piece{}", "piece {\n}piece\t{ }"],
            ParseTree {
                pieces: vec![Piece::default(), Piece::default()],
            },
        );
    }

    #[test]
    fn parse_piece_with_attributes() {
        parse_equivalent(
            &[
                "piece { tempo: 120, beats: 4 }",
                "piece {
                    tempo: 120,
                    beats: 4,
                 }",
                "tempo: 120
                 beats: 4
                ",
            ],
            ParseTree {
                pieces: vec![Piece {
                    tempo: Some(120),
                    beats: Some(4),
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_toplevel_piece_attributes() {
        parse_succeeds(
            r#"title: "Title", composer: "Composer""#,
            ParseTree {
                pieces: vec![Piece {
                    title: Some(b"Title"),
                    composer: Some(b"Composer"),
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_piece_with_all_attributes() {
        parse_succeeds(
            r#"piece {
                title: "Title",
                composer: "Composer",
                tempo: 100,
                beats: 3,
             }"#,
            ParseTree {
                pieces: vec![Piece {
                    title: Some(b"Title"),
                    composer: Some(b"Composer"),
                    tempo: Some(100),
                    beats: Some(3),
                    plays: vec![],
                    voices: vec![],
                }],
            },
        );
    }

    #[test]
    fn fail_unopened_piece() {
        parse_fails("piece");
    }

    #[test]
    fn fail_unclosed_piece() {
        parse_fails("piece {");
    }

    #[test]
    fn fail_unexpected_token_piece() {
        parse_fails("piece @");
    }

    #[test]
    fn parse_piece_with_anon_empty_voice() {
        parse_succeeds(
            "piece { voice { } }",
            ParseTree {
                pieces: vec![Piece {
                    voices: vec![Voice::default()],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_piece_with_anon_empty_play() {
        parse_succeeds("piece { play { } }", plays_tree(&[Play::default()]));
    }

    #[test]
    fn parse_piece_with_anon_empty_voice_and_play() {
        parse_equivalent(
            &[
                "piece { play { } voice { } }",
                "piece { voice { } play { } }",
            ],
            ParseTree {
                pieces: vec![Piece {
                    plays: vec![Play::default()],
                    voices: vec![Voice::default()],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_solo_anon_empty_voice() {
        parse_succeeds(
            "voice { }",
            ParseTree {
                pieces: vec![Piece {
                    voices: vec![Voice::default()],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_solo_anon_empty_play() {
        parse_succeeds("play { }", plays_tree(&[Play::default()]));
    }

    #[test]
    fn parse_solo_named_play() {
        parse_equivalent(
            &[
                "play Named {}",
                "play Named{}",
                "play Named
                 {
                 }",
            ],
            plays_tree(&[Play {
                name: Some(b"Named"),
                ..Play::default()
            }]),
        );
    }

    #[test]
    fn parse_solo_named_voice() {
        parse_equivalent(
            &[
                "voice Named {}",
                "voice Named{}",
                "voice Named
                 {
                 }",
            ],
            ParseTree {
                pieces: vec![Piece {
                    voices: vec![Voice {
                        name: Some(b"Named"),
                        ..Voice::default()
                    }],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_solo_anon_empty_voice_and_play() {
        parse_equivalent(
            &["play { } voice { }", "voice { } play { }"],
            ParseTree {
                pieces: vec![Piece {
                    plays: vec![Play::default()],
                    voices: vec![Voice::default()],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_voice_with_single_attribute() {
        parse_equivalent(
            &[
                "voice { program:10 }",
                "voice { program: 10 }",
                "voice { program: 10, }",
                "voice {
                    program: 10
                }",
                "voice {
                    program: 10,
                }",
            ],
            ParseTree {
                pieces: vec![Piece {
                    voices: vec![Voice {
                        program: Some(10),
                        ..Voice::default()
                    }],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_voice_with_multiple_attributes() {
        parse_equivalent(
            &[
                "voice { program: 30, channel: 2 }",
                "voice { program: 30, channel: 2, }",
                "voice { program: 30; channel: 2; }",
                "voice { program: 30
                    channel: 2, }",
                "voice {
                    program: 30
                    channel: 2
                }",
                "voice {
                    program: 30,
                    channel: 2,
                }",
            ],
            ParseTree {
                pieces: vec![Piece {
                    voices: vec![Voice {
                        program: Some(30),
                        channel: Some(2),
                        ..Voice::default()
                    }],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_voice_with_all_attributes() {
        parse_succeeds(
            "voice {
                octave: -2,
                channel: 3,
                program: 5,
                volume: 8,
                drums: true,
            }",
            ParseTree {
                pieces: vec![Piece {
                    voices: vec![Voice {
                        transpose: Some(-24),
                        channel: Some(3),
                        program: Some(5),
                        volume: Some(8),
                        drums: Some(true),
                        name: None,
                    }],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn parse_play_with_one_grand_stave_and_one_basic_stave() {
        parse_equivalent(
            &[
                "play { :| }",
                "play { : | }",
                "play { :
                    |
                }",
                "play {
                    :|
                }",
            ],
            plays_tree(&[Play {
                grand_staves: vec![GrandStave {
                    staves: vec![Stave { prefix: None }],
                }],
                ..Play::default()
            }]),
        );
    }

    #[test]
    fn parse_play_with_one_grand_stave_and_two_basic_staves() {
        parse_equivalent(
            &[
                "play {
                    :| ; :|
                }",
                "play {
                    :|
                    :|
                }",
            ],
            plays_tree(&[Play {
                grand_staves: vec![GrandStave {
                    staves: vec![Stave { prefix: None }, Stave { prefix: None }],
                }],
                ..Play::default()
            }]),
        );
    }

    #[test]
    fn parse_play_with_two_grand_staves() {
        parse_equivalent(
            &[
                "play { :| ;; :| }",
                "play { :| ; ; :| }",
                //                 "play { :| ;;; :| }", // TODO: This fails because a line starts with `; How should that be handled?
                "play {
                    :| ;
                    :|
                }",
                "play {
                    :|

                    :|
                }",
                "play {
                    :|




                    :|
                }",
            ],
            plays_tree(&[Play {
                grand_staves: vec![
                    GrandStave {
                        staves: vec![Stave { prefix: None }],
                    },
                    GrandStave {
                        staves: vec![Stave { prefix: None }],
                    },
                ],
                ..Play::default()
            }]),
        );
    }

    #[test]
    fn parse_solo_stave_as_play_block() {
        parse_succeeds(
            ":|",
            plays_tree(&[Play {
                grand_staves: vec![GrandStave {
                    staves: vec![Stave { prefix: None }],
                }],
                ..Play::default()
            }]),
        );
    }

    #[test]
    fn parse_solo_two_staves() {
        parse_equivalent(
            &[
                ":|
                 :|",
                ":| ; :|",
                ":|
                  |
                 :|",
            ],
            plays_tree(&[Play {
                grand_staves: vec![GrandStave {
                    staves: vec![Stave { prefix: None }, Stave { prefix: None }],
                }],
                ..Play::default()
            }]),
        );
    }

    #[test]
    fn parse_toplevel_piece_attributes_and_toplevel_staves() {
        parse_succeeds(
            r#"tempo: 160
               beats: 6

               :|
            "#,
            ParseTree {
                pieces: vec![Piece {
                    tempo: Some(160),
                    beats: Some(6),
                    plays: vec![Play {
                        grand_staves: vec![GrandStave {
                            staves: vec![Stave { prefix: None }],
                        }],
                        ..Play::default()
                    }],
                    ..Piece::default()
                }],
            },
        );
    }

    #[test]
    fn comments_are_whitespace() {
        parse_equivalent(
            &[
                "play PlayName { :| ;; :| ; :| } // Comment at end",
                "play PlayName { // Comments
                    :|           // in
                                 // every
                    :| ; :|      // line
                }",
                "play // Comments on
                 PlayName // some of the
                 {
                    :|

                    :|
                    :|
                 } // lines
                ",
            ],
            plays_tree(&[Play {
                name: Some(b"PlayName"),
                grand_staves: vec![
                    GrandStave {
                        staves: vec![Stave { prefix: None }],
                    },
                    GrandStave {
                        staves: vec![Stave { prefix: None }, Stave { prefix: None }],
                    },
                ],
                ..Play::default()
            }]),
        );
    }
}
