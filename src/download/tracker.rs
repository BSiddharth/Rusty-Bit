pub enum Event {
    //The first request to the tracker must include the event key with this value.
    STARTED,

    //Must be sent to the tracker if the client is shutting down gracefully.
    STOPPED,

    // Must be sent to the tracker when the download completes. However, must not be sent if the download was already 100% complete when the client started. Presumably, this is to allow the tracker to increment the "completed downloads" metric based solely on this event.
    COMPLETED,
}

pub struct TrackerRequest {
    // urlencoded 20-byte SHA1 hash of the value of the info key from the Metainfo file. Note that the value will be a bencoded dictionary, given the definition of the info key above.
    pub info_hash: [u8; 20],

    // urlencoded 20-byte string used as a unique ID for the client, generated by the client at startup. This is allowed to be any value, and may be binary data. There are currently no guidelines for generating this peer ID. However, one may rightly presume that it must at least be unique for your local machine, thus should probably incorporate things like process ID and perhaps a timestamp recorded at startup. See peer_id below for common client encodings of this field.
    pub peer_id: [u8; 20],

    // The port number that the client is listening on. Ports reserved for BitTorrent are typically 6881-6889. Clients may choose to give up if it cannot establish a port within this range.
    pub port: u16,

    //  The total amount uploaded (since the client sent the 'started' event to the tracker) in base ten ASCII. While not explicitly stated in the official specification, the concensus is that this should be the total number of bytes uploaded.
    pub uploaded: usize,

    // The total amount downloaded (since the client sent the 'started' event to the tracker) in
    // base ten ASCII. While not explicitly stated in the official specification, the consensus is
    // that this should be the total number of bytes downloaded.
    pub downloaded: usize,

    // The number of bytes this client still has to download in base ten ASCII. Clarification: The number of bytes needed to download to be 100% complete and get all the included files in the torrent.
    pub left: usize,

    //Setting this to 1 indicates that the client accepts a compact response. The peers list is replaced by a peers string with 6 bytes per peer. The first four bytes are the host (in network byte order), the last two bytes are the port (again in network byte order). It should be noted that some trackers only support compact responses (for saving bandwidth) and either refuse requests without "compact=1" or simply send a compact response unless the request contains "compact=0" (in which case they will refuse the request.)
    pub compact: u8,

    //Indicates that the tracker can omit peer id field in peers dictionary. This option is ignored if compact is enabled.
    pub no_peer_id: usize,

    // If specified, must be one of started, completed, stopped, (or empty which is the same as not being specified). If not specified, then this request is one performed at regular intervals.
    pub event: Event,
}
