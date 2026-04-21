//! Renet channel configuration for voxel-world multiplayer.
//!
//! This module defines the communication channels used for networking,
//! each with different delivery guarantees suited to specific data types.

use renet::{ChannelConfig, SendType};
use std::time::Duration;

/// Channel identifiers for different message types.
/// Each channel has different delivery guarantees optimized for its data type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Channel {
    /// Player movement updates (position, velocity, rotation).
    /// Unreliable - dropped packets are acceptable, latest state is most important.
    /// Sent frequently (~20/sec) for smooth movement.
    PlayerMovement = 0,

    /// Block placement and breaking operations.
    /// Reliable unordered - must arrive, but order doesn't matter.
    /// Block changes are critical but independent.
    BlockUpdates = 1,

    /// Game state messages (join, leave, chat, time sync).
    /// Reliable ordered - must arrive in correct sequence.
    /// These messages affect game state and must be processed in order.
    GameState = 2,

    /// Chunk data streaming.
    /// Reliable-unordered — chunks are large but must not be silently lost
    /// (a missed chunk leaves the client with a hole in the world). Order
    /// within the stream doesn't matter since each chunk is self-contained.
    ChunkStream = 3,
}

impl Channel {
    /// Returns all channel variants.
    pub const fn all() -> [Channel; 4] {
        [
            Channel::PlayerMovement,
            Channel::BlockUpdates,
            Channel::GameState,
            Channel::ChunkStream,
        ]
    }

    /// Returns the channel ID as u8.
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// Default resend time for reliable channels.
const RESEND_TIME: Duration = Duration::from_millis(200);

/// Creates the channel configuration for renet.
/// Returns (server_channels, client_channels).
pub fn create_channels() -> (Vec<ChannelConfig>, Vec<ChannelConfig>) {
    let server_channels = vec![
        // PlayerMovement: Unreliable, fast updates
        ChannelConfig {
            channel_id: Channel::PlayerMovement.id(),
            max_memory_usage_bytes: 5 * 1024 * 1024, // 5 MB
            send_type: SendType::Unreliable,
        },
        // BlockUpdates: Reliable unordered
        ChannelConfig {
            channel_id: Channel::BlockUpdates.id(),
            max_memory_usage_bytes: 10 * 1024 * 1024, // 10 MB
            send_type: SendType::ReliableUnordered {
                resend_time: RESEND_TIME,
            },
        },
        // GameState: Reliable ordered
        ChannelConfig {
            channel_id: Channel::GameState.id(),
            max_memory_usage_bytes: 5 * 1024 * 1024, // 5 MB
            send_type: SendType::ReliableOrdered {
                resend_time: RESEND_TIME,
            },
        },
        // ChunkStream: reliable-unordered so chunks aren't silently lost on
        // packet drop. renet enforces `max_memory_usage_bytes` *per channel
        // per connected client*; tightening from the previous 50 MiB to
        // 8 MiB caps the server-wide worst case at 32 MiB for 4 clients —
        // still covers the full view-volume (729 chunks × ~4 KB) plus
        // generous slack for an initial-join download storm without leaving
        // a long memory tail.
        ChannelConfig {
            channel_id: Channel::ChunkStream.id(),
            max_memory_usage_bytes: 8 * 1024 * 1024,
            send_type: SendType::ReliableUnordered {
                resend_time: RESEND_TIME,
            },
        },
    ];

    // Client uses same channels
    let client_channels = server_channels.clone();

    (server_channels, client_channels)
}

/// Creates a ConnectionConfig with our channels.
pub fn create_connection_config() -> renet::ConnectionConfig {
    let (server_channels, client_channels) = create_channels();

    renet::ConnectionConfig {
        available_bytes_per_tick: 60_000, // 60 KB per tick at 60Hz = ~28.8 Mbps
        server_channels_config: server_channels,
        client_channels_config: client_channels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_ids_are_unique() {
        let ids: Vec<u8> = Channel::all().iter().map(|c| c.id()).collect();
        for (i, id) in ids.iter().enumerate() {
            for (j, other) in ids.iter().enumerate() {
                if i != j {
                    assert_ne!(id, other, "Channel IDs must be unique");
                }
            }
        }
    }

    #[test]
    fn test_create_channels_count() {
        let (server, client) = create_channels();
        assert_eq!(server.len(), 4);
        assert_eq!(client.len(), 4);
    }
}
