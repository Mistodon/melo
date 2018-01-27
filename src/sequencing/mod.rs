pub mod data;
pub mod error;


use parsing::data::*;
use trust::Trust;

use self::data::*;
use self::error::{ SequencingError, ErrorType };


pub fn sequence_pieces<'a>(
    piece_nodes: &[PieceNode<'a>]) -> Result<Vec<Piece<'a>>, SequencingError>
{
    use notes::lcm;

    let mut pieces = Vec::new();

    for piece_node in piece_nodes
    {
        // validation
        {
            for play in &piece_node.plays
            {
                let matched = piece_node.voices.iter().any(|voice| Some(voice.name) == play.voice);
                if !matched
                {
                    let error = match play.voice
                    {
                        Some(voice_name) => ErrorType::UndeclaredVoice { voice_name: voice_name.to_owned() },
                        None => ErrorType::VoicelessPlayBlock
                    };

                    // TODO(***realname***): This error needs file position information
                    return Err(SequencingError { line: 123456789, col: 123456789, error })
                }
            }
        }

        // TODO(***realname***): Stop this - make things immutable after construction so new fields aren't left out
        let mut piece = Piece::default();

        piece.title = piece_node.title.or(piece.title);
        piece.composer = piece_node.composer.or(piece.composer);
        piece.tempo = piece_node.tempo.unwrap_or(piece.tempo);
        piece.beats = piece_node.beats.unwrap_or(piece.beats);

        for voice_node in &piece_node.voices
        {
            let mut voice = Voice { name: voice_node.name, .. Default::default() };
            voice.channel = voice_node.channel.unwrap_or(voice.channel);
            voice.program = voice_node.program.unwrap_or(voice.program);
            voice.octave = voice_node.octave.unwrap_or(voice.octave);
            voice.volume = voice_node.volume.map(|vol| f64::from(vol) / 127.0);

            let bar_length_lcm = piece_node.plays
                .iter()
                .filter(|play| play.voice == Some(voice.name))
                .flat_map(
                    |play| play.staves.iter()
                        .flat_map(
                            |stave| stave.bars.iter()
                                .map(|bar| bar.notes.iter().map(|note| note.length()).sum())))
                .fold(1, lcm);

            voice.divisions_per_bar = bar_length_lcm;


            for play_node in &piece_node.plays
            {
                if play_node.voice != Some(voice.name)
                {
                    continue
                }

                for stave_node in &play_node.staves
                {
                    let mut previous_note_exists = false;

                    for (index, bar_node) in stave_node.bars.iter().enumerate()
                    {
                        let mut cursor = index as u32 * voice.divisions_per_bar;

                        let bar_node_length: u32 = bar_node.notes.iter().map(|note| note.length()).sum();

                        assert!(voice.divisions_per_bar % bar_node_length == 0);
                        let note_scale = voice.divisions_per_bar / bar_node_length;

                        for note_node in &bar_node.notes
                        {
                            match *note_node
                            {
                                NoteNode::Rest { length } => {
                                    previous_note_exists = false;
                                    cursor += note_scale * u32::from(length);
                                }
                                NoteNode::Extension { length } => {
                                    if previous_note_exists
                                    {
                                        let previous_note = voice.notes.last_mut().trust();
                                        previous_note.length += note_scale * u32::from(length);
                                    }

                                    cursor += note_scale * u32::from(length);
                                }
                                NoteNode::Note { midi, length } => {
                                    previous_note_exists = true;

                                    let octave = voice.octave;

                                    let midi = midi.checked_add(octave * 12).ok_or_else(
                                        || SequencingError
                                        {
                                            line: 12345,
                                            col: 12345,
                                            error: ErrorType::InvalidNote { midi, octave_offset: octave }
                                        })?;

                                    let length = note_scale * u32::from(length);
                                    let position = cursor;
                                    let note = Note { midi, length, position };

                                    voice.notes.push(note);

                                    cursor += length;
                                }
                            }
                        }
                    }
                }

                voice.notes.sort_by_key(|note| note.position);
            }

            piece.voices.push(voice);
        }

        pieces.push(piece);
    }

    Ok(pieces)
}


#[cfg(test)]
mod tests
{
    use super::*;
    use lexing;
    use parsing;


    fn sequence_test(source: &str, expected: Piece)
    {
        let tokens = &lexing::lex(source).expect("ERROR IN LEXER");
        let parse_tree = parsing::parse(tokens).expect("ERROR IN PARSER");
        let piece = &sequence_pieces(&parse_tree.pieces).unwrap()[0];
        assert_eq!(piece, &expected);
    }

    fn sequence_test_fail(source: &str)
    {
        let tokens = &lexing::lex(source).expect("ERROR IN LEXER");
        let parse_tree = parsing::parse(tokens).expect("ERROR IN PARSER");
        assert!(sequence_pieces(&parse_tree.pieces).is_err());
    }

    fn voice_test(source: &str, expected_notes: Vec<Note>)
    {
        let tokens = &lexing::lex(source).expect("ERROR IN LEXER");
        let parse_tree = parsing::parse(tokens).expect("ERROR IN PARSER");
        let piece = &sequence_pieces(&parse_tree.pieces).unwrap()[0];
        assert_eq!(piece.voices[0].notes, expected_notes);
    }

    #[test]
    fn sequence_empty_piece()
    {
        sequence_test("", Piece::default());
    }

    #[test]
    fn piece_with_attributes()
    {
        sequence_test("piece { title: One, composer: Two, tempo: 3, beats: 4 }",
            Piece
            {
                title: Some("One"),
                composer: Some("Two"),
                tempo: 3,
                beats: 4,
                .. Default::default()
            });
    }

    #[test]
    fn piece_with_empty_voice()
    {
        sequence_test("voice Empty { }",
            Piece
            {
                voices: vec![Voice { name: "Empty", .. Default::default() }],
                .. Default::default()
            });
    }

    #[test]
    fn voice_with_mismatched_play()
    {
        sequence_test_fail("voice OneNote { } play Different { :| C }");
    }

    #[test]
    fn voice_with_single_note()
    {
        voice_test("voice OneNote { } play OneNote { :| C }", vec![Note { midi: 60, length: 1, position: 0 }]);
    }

    #[test]
    fn voice_with_two_notes()
    {
        voice_test("voice TwoNote { } play TwoNote { :| C G }",
           vec![
               Note { midi: 60, length: 1, position: 0 },
               Note { midi: 67, length: 1, position: 1 },
           ]);
    }

    #[test]
    fn voice_with_two_staves()
    {
        voice_test("voice Diad { } play Diad { :| C ; :| G }",
           vec![
               Note { midi: 60, length: 1, position: 0 },
               Note { midi: 67, length: 1, position: 0 },
           ]);
    }

    #[test]
    fn threes_against_twos()
    {
        voice_test("voice Diad { } play Diad { :| C E G ; :| c g }",
           vec![
               Note { midi: 60, length: 2, position: 0 },
               Note { midi: 72, length: 3, position: 0 },
               Note { midi: 64, length: 2, position: 2 },
               Note { midi: 79, length: 3, position: 3 },
               Note { midi: 67, length: 2, position: 4 },
           ]);
    }

    #[test]
    fn fail_when_notes_moved_out_of_range()
    {
        sequence_test_fail("voice V { octave: 1} play V { :| g^'''}");
    }

    #[test]
    fn voice_with_note_lengths()
    {
        voice_test("voice A { } play A { :| C4 -2 G2 }",
           vec![
               Note { midi: 60, length: 4, position: 0 },
               Note { midi: 67, length: 2, position: 6 },
           ]);
    }

    #[test]
    fn voice_with_dots()
    {
        voice_test("voice A { } play A { :| A..B C... .8 }",
            vec![
                Note { midi: 57, length: 3, position: 0 },
                Note { midi: 59, length: 1, position: 3 },
                Note { midi: 60, length: 12, position: 4 },
            ]);
    }

    #[test]
    fn voice_with_leading_dots()
    {
        voice_test("voice A { } play A { :| ...C E... G... -... }",
            vec![
                Note { midi: 60, length: 1, position: 3 },
                Note { midi: 64, length: 4, position: 4 },
                Note { midi: 67, length: 4, position: 8 },
            ]);
    }

    #[test]
    fn dots_do_not_carry_across_staves()
    {
        voice_test("voice A { } play A { :| CEGc | ; :| ...g }",
            vec![
                Note { midi: 60, length: 1, position: 0 },
                Note { midi: 64, length: 1, position: 1 },
                Note { midi: 67, length: 1, position: 2 },
                Note { midi: 72, length: 1, position: 3 },
                Note { midi: 79, length: 1, position: 3 },
            ]);
    }

    #[test]
    fn notes_can_be_tied_across_bars()
    {
        voice_test("voice A {} play A { :| CEG. | ..EC }",
            vec![
                Note { midi: 60, length: 1, position: 0 },
                Note { midi: 64, length: 1, position: 1 },
                Note { midi: 67, length: 4, position: 2 },
                Note { midi: 64, length: 1, position: 6 },
                Note { midi: 60, length: 1, position: 7 },
            ]);
    }
}
