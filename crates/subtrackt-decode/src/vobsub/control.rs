//! VOBSUB control-sequence opcodes.
//!
//! Parsing is #3; the opcode table is here because it is the format's actual interface. A
//! subpicture is a pixel blob plus a chain of control sequences, each with a delay, each holding
//! commands from this set.

/// A command inside a control sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    /// `0x00` — force display, used for forced/foreign-dialogue subtitles.
    ForcedStartDisplay,
    /// `0x01` — start displaying at this sequence's delay.
    StartDisplay,
    /// `0x02` — stop displaying at this sequence's delay.
    StopDisplay,
    /// `0x03` — select four palette indices out of the 16-colour sidecar palette.
    SetPalette,
    /// `0x04` — set the alpha of each of the four selected indices.
    SetAlpha,
    /// `0x05` — set the display area within the subtitle plane.
    SetDisplayArea,
    /// `0x06` — offsets of the top-field and bottom-field pixel data.
    SetPixelOffsets,
    /// `0x07` — mid-subtitle colour/contrast change, used for karaoke-style wipes.
    ChangeColorContrast,
    /// `0xFF` — end of this control sequence.
    End,
}

impl Command {
    /// Map an opcode byte, or `None` for one the format does not define.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::ForcedStartDisplay),
            0x01 => Some(Self::StartDisplay),
            0x02 => Some(Self::StopDisplay),
            0x03 => Some(Self::SetPalette),
            0x04 => Some(Self::SetAlpha),
            0x05 => Some(Self::SetDisplayArea),
            0x06 => Some(Self::SetPixelOffsets),
            0x07 => Some(Self::ChangeColorContrast),
            0xFF => Some(Self::End),
            _ => None,
        }
    }

    /// Length of this command's operands, or `None` for the variable-length
    /// [`Command::ChangeColorContrast`], whose operands are prefixed with their own size.
    #[must_use]
    pub const fn operand_len(self) -> Option<usize> {
        match self {
            Self::ForcedStartDisplay | Self::StartDisplay | Self::StopDisplay | Self::End => {
                Some(0)
            }
            Self::SetPalette | Self::SetAlpha => Some(2),
            Self::SetPixelOffsets => Some(4),
            Self::SetDisplayArea => Some(6),
            Self::ChangeColorContrast => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_map_to_commands() {
        assert_eq!(Command::from_byte(0x01), Some(Command::StartDisplay));
        assert_eq!(Command::from_byte(0xFF), Some(Command::End));
        assert_eq!(Command::from_byte(0x42), None);
    }

    #[test]
    fn operand_lengths_match_the_spec() {
        assert_eq!(Command::StopDisplay.operand_len(), Some(0));
        assert_eq!(Command::SetPalette.operand_len(), Some(2));
        assert_eq!(Command::SetDisplayArea.operand_len(), Some(6));
        // The wipe command is self-describing and cannot be skipped by a fixed width.
        assert_eq!(Command::ChangeColorContrast.operand_len(), None);
    }
}
