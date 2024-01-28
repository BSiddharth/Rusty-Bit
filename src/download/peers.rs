use anyhow::bail;
use serde::{Deserialize, Serialize};
use tokio_util::{
    bytes::{Buf, BytesMut},
    codec::{Decoder, Encoder},
};

#[repr(u8)]
#[derive(Debug, Serialize, Deserialize)]
pub enum PeerMsgTag {
    // The keep-alive message is a message with zero bytes, specified with the length prefix set to zero.
    // There is no message ID and no payload.
    // Peers may close a connection if they receive no messages (keep-alive or any other message) for
    // a certain period of time, so a keep-alive message must be sent to maintain the connection alive
    // if no command have been sent for a given amount of time.
    // This amount of time is generally two minutes.
    // <len=0000>
    // KeepAlive,

    // The choke message is fixed-length and has no payload.
    // <len=0001><id=0>
    Choke,

    // The unchoke message is fixed-length and has no payload.
    // <len=0001><id=1>
    Unchoke,

    // The interested message is fixed-length and has no payload.
    // <len=0001><id=2>
    Interested,

    // The not interested message is fixed-length and has no payload.
    // <len=0001><id=3>
    NotInterested,

    // The have message is fixed length.
    // The payload is the zero-based index of a piece that has just been successfully downloaded and verified via the hash.
    // <len=0005><id=4><piece index>
    Have,

    // The bitfield message may only be sent immediately after the handshaking sequence is completed,
    // and before any other messages are sent. It is optional, and need not be sent if a client has no pieces.
    // The bitfield message is variable length, where X is the length of the bitfield.
    // The payload is a bitfield representing the pieces that have been successfully downloaded.
    // The high bit in the first byte corresponds to piece index 0.
    // Bits that are cleared indicated a missing piece, and set bits indicate a valid and available piece.
    // Spare bits at the end are set to zero.

    // Some clients (Deluge for example) send bitfield with missing pieces even if it has all data.
    // Then it sends rest of pieces as have messages.
    // They are saying this helps against ISP filtering of BitTorrent protocol. It is called lazy bitfield.

    // A bitfield of the wrong length is considered an error.
    // Clients should drop the connection if they receive bitfields that are not of the correct size,
    // or if the bitfield has any of the spare bits set.

    // <len=0001+X><id=5><bitfield>
    Bitfield,

    // The request message is fixed length, and is used to request a block. The payload contains the following information:

    // index: integer specifying the zero-based piece index
    // begin: integer specifying the zero-based byte offset within the piece
    // length: integer specifying the requested length.
    // <len=0013><id=6><index><begin><length>
    Request,

    // The piece message is variable length, where X is the length of the block. The payload contains the following information:

    // index: integer specifying the zero-based piece index
    // begin: integer specifying the zero-based byte offset within the piece
    // block: block of data, which is a subset of the piece specified by index.
    // <len=0009+X><id=7><index><begin><block>
    Piece,

    // The cancel message is fixed length, and is used to cancel block requests.
    // The payload is identical to that of the "request" message.
    // It is typically used during "End Game".
    // <len=0013><id=8><index><begin><length>
    Cancel,
}

impl TryFrom<u8> for PeerMsgTag {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, &'static str> {
        match value {
            // 0 => Ok(PeerMsgType::KeepAlive),
            0 => Ok(PeerMsgTag::Choke),
            1 => Ok(PeerMsgTag::Unchoke),
            2 => Ok(PeerMsgTag::Interested),
            3 => Ok(PeerMsgTag::NotInterested),
            4 => Ok(PeerMsgTag::Have),
            5 => Ok(PeerMsgTag::Bitfield),
            6 => Ok(PeerMsgTag::Request),
            7 => Ok(PeerMsgTag::Piece),
            8 => Ok(PeerMsgTag::Cancel),
            _ => Err("Conversion of u8 to PeerMsgType not possible"),
        }
    }
}
//
// impl TryInto<u8> for PeerMsgTag {
//     type Error = &'static str;
//     fn try_into(self) -> Result<u8, &'static str> {
//         match self {
//             PeerMsgTag::Choke => Ok(0),
//             PeerMsgTag::Unchoke => Ok(1),
//             PeerMsgTag::Interested => Ok(2),
//             PeerMsgTag::NotInterested => Ok(3),
//             PeerMsgTag::Have => Ok(4),
//             PeerMsgTag::Bitfield => Ok(5),
//             PeerMsgTag::Request => Ok(6),
//             PeerMsgTag::Piece => Ok(7),
//             PeerMsgTag::Cancel => Ok(8),
//             _ => Err("Conversion of PeerMsgType to u8 not possible"),
//         }
//     }
// }

#[derive(Serialize, Deserialize, Debug)]
pub struct PeerMsgType {
    msg_length: u32,
    tag: PeerMsgTag,
    data: Vec<u8>,
}

impl PeerMsgType {
    pub fn new(tag: PeerMsgTag, data: Vec<u8>) -> PeerMsgType {
        return PeerMsgType {
            msg_length: (data.len() + 1) as u32,
            tag,
            data,
        };
    }
}

pub struct PeerFrameCodec;

const MAX: usize = 1024 * 16; // 16KB for now is the max len that is allowed in the protocol

impl Decoder for PeerFrameCodec {
    type Item = PeerMsgType;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> anyhow::Result<Option<Self::Item>> {
        if src.len() < 4 {
            // Not enough data to read length marker.
            return Ok(None);
        }

        // Read length marker.
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes) as usize;

        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if length > MAX {
            bail!("Frame of length {} is too large.", length);
        }

        if src.len() < 4 + length {
            // The full data has not yet arrived.

            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            src.reserve(4 + length - src.len());

            // We inform the Framed that we need more bytes to form the next
            // frame.
            return Ok(None);
        }
        println!("{}", length);

        if length == 0 {
            src.advance(4);
            println!("Keep Alive received");
            return self.decode(src);
        };

        let msg_type: u8 = src[4];

        if length == 1 {
            src.advance(4 + length);
            return Ok(Some(PeerMsgType::new(
                PeerMsgTag::try_from(msg_type).unwrap(),
                Vec::new(),
            )));
        };

        let data = src[5..4 + length].to_vec();
        src.advance(4 + length);
        return Ok(Some(PeerMsgType::new(
            PeerMsgTag::try_from(msg_type).unwrap(),
            data,
        )));

        //     match PeerMsgTag::try_from(msg_type).unwrap() {
        //         PeerMsgTag::KeepAlive => bail!("Msg Type not possible"),
        //         PeerMsgTag::Choke => return Ok(Some(PeerMsgTag::Choke)),
        //         PeerMsgTag::Unchoke => return Ok(Some(PeerMsgTag::Unchoke)),
        //         PeerMsgTag::Interested => return Ok(Some(PeerMsgTag::Interested)),
        //         PeerMsgTag::NotInterested => return Ok(Some(PeerMsgTag::NotInterested)),
        //         PeerMsgTag::Have => bail!("Msg Type not possible"),
        //         PeerMsgTag::Bitfield => bail!("Msg Type not possible"),
        //         PeerMsgTag::Request => bail!("Msg Type not possible"),
        //         PeerMsgTag::Piece => bail!("Msg Type not possible"),
        //         PeerMsgTag::Cancel => bail!("Msg Type not possible"),
        //     }
        // }
        //
        // // Use advance to modify src such that it no longer contains
        // // this frame.
        // if length > 1 {
        //     // let data = src[5..5 + length].to_vec();
        //     src.advance(4 + length);
        //     match PeerMsgTag::try_from(msg_type).unwrap() {
        //         PeerMsgTag::KeepAlive => bail!("Msg Type not possible"),
        //         PeerMsgTag::Choke => bail!("Msg Type not possible"),
        //         PeerMsgTag::Unchoke => bail!("Msg Type not possible"),
        //         PeerMsgTag::Interested => bail!("Msg Type not possible"),
        //         PeerMsgTag::NotInterested => bail!("Msg Type not possible"),
        //         PeerMsgTag::Have => return Ok(Some(PeerMsgTag::Have)),
        //         PeerMsgTag::Bitfield => return Ok(Some(PeerMsgTag::Bitfield)),
        //         PeerMsgTag::Request => return Ok(Some(PeerMsgTag::Request)),
        //         PeerMsgTag::Piece => return Ok(Some(PeerMsgTag::Piece)),
        //         PeerMsgTag::Cancel => return Ok(Some(PeerMsgTag::Cancel)),
        //     }
        // } else {
        //     bail!("Not possible");
        // }
    }
}

impl Encoder<PeerMsgType> for PeerFrameCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: PeerMsgType, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Don't send a string if it is longer than the other end will
        // accept.
        // if item.len() > MAX {
        //     return Err(std::io::Error::new(
        //         std::io::ErrorKind::InvalidData,
        //         format!("Frame of length {} is too large.", item.len()),
        //     ));
        // }

        // Convert the length into a byte array.
        // The cast to u32 cannot overflow due to the length check above.
        let len_slice = u32::to_be_bytes(item.msg_length);
        let msg_type_slice = u8::to_be_bytes(item.tag as u8);
        let data = item.data.as_slice();

        // Reserve space in the buffer.
        dst.reserve(len_slice.len() + msg_type_slice.len() + data.len());

        // Write the length and string to the buffer.
        dst.extend(len_slice);
        dst.extend(msg_type_slice);
        dst.extend(data);
        Ok(())
    }
}

pub struct PeerRequestMsgType {
    // The request message is fixed length, and is used to request a block. The payload contains the following information:

    // integer specifying the zero-based piece index
    index: u32,
    // integer specifying the zero-based byte offset within the piece
    begin: u32,
    //  integer specifying the requested length.
    length: u32, // <len=0013><id=6><index><begin><length>
}

impl PeerRequestMsgType {
    pub fn new(index: u32, begin: u32, length: u32) -> PeerRequestMsgType {
        PeerRequestMsgType {
            index,
            begin,
            length,
        }
    }
    pub fn to_bytes(self) -> [u8; 12] {
        let mut bytes = Vec::with_capacity(12);
        bytes.extend(self.index.to_be_bytes());
        bytes.extend(self.begin.to_be_bytes());
        bytes.extend(self.length.to_be_bytes());
        bytes.try_into().unwrap()
    }
}
