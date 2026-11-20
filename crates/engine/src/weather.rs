//! Client-side weather: `NetHandlerPlayClient.handleChangeGameState` of 1.8.9.
//!
//! The vanilla client never runs `World.updateWeatherBody`: it returns at once when `World.isRemote`
//! (checked with `javap`). The strengths change only by Change Game State packets, so a thunder strength
//! the server sent stays until the next packet; the server sends one on every change.

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Weather {
    rain: f32,
    thunder: f32,
}

impl Weather {
    /// Change Game State: 1 begin rain, 2 end rain, 7 rain strength, 8 thunder strength; other reasons are not weather.
    pub fn apply(&mut self, reason: u8, value: f32) {
        match reason {
            1 => self.rain = 0.0,
            2 => self.rain = 1.0,
            7 => self.rain = value,
            8 => self.thunder = value,
            _ => {}
        }
    }

    /// A new `WorldClient` starts dry: rain 0, thunder 0 (`World` field defaults).
    pub fn reset(&mut self) {
        *self = Weather::default();
    }

    /// `World.getRainStrength` and `getThunderStrength`.
    pub fn strengths(&self) -> (f32, f32) {
        (self.rain, self.thunder)
    }
}

/// Per column: the precipitation height (the y above the highest rain-stopping block) and the biome id;
/// `None` for an unloaded column.
pub(crate) fn precipitation_top(
    store: &world::chunk::ChunkStore,
) -> impl Fn(i32, i32) -> Option<(i32, u8)> + '_ {
    move |x, z| {
        let biome = store
            .get(x >> 4, z >> 4)?
            .biome(x.rem_euclid(16) as usize, z.rem_euclid(16) as usize);
        let y = (0..256)
            .rev()
            .find(|&y| render::stops_rain(store.get_block(x, y, z)))
            .map_or(0, |y| y + 1);
        Some((y, biome))
    }
}

/// `World.isRaining(pos)` with `canSeeSky` shortcut as the precipitation height: raining, open above and a rainy warm biome.
pub(crate) fn raining_at(
    store: &world::chunk::ChunkStore,
    rain: f32,
) -> impl Fn(i32, i32, i32) -> bool + '_ {
    let top = precipitation_top(store);
    move |x, y, z| {
        rain > 0.2
            && top(x, z)
                .is_some_and(|(height, biome)| height <= y && render::splashes(biome, x, y, z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: vanilla 1.8.9 `World.getCloudColor` at noon (day scale 1) with rain 1, thunder 0:
    /// each channel is 1*(1-0.95) + 0.6*0.95 = 0.62, a faint grey, never navy.
    #[test]
    fn snow_packets_give_vanilla_grey_clouds() {
        let mut w = Weather::default();
        w.apply(1, 0.0);
        w.apply(7, 1.0);
        w.apply(8, 0.0);
        let (rain, thunder) = w.strengths();
        let env = render::Environment {
            rain,
            thunder,
            ..Default::default()
        };
        let cloud = env.sky_state(0.0, [0.0, 0.0, 1.0]).cloud;
        for c in cloud {
            assert!((c - 0.62).abs() < 1e-3, "{cloud:?}");
        }
    }

    /// Oracle: vanilla 1.8.9 getCloudColor; the server thunder strength is used as sent,
    /// so thunder 1 at noon gives 0.62*0.05 + 0.62*0.2*0.95 per channel.
    #[test]
    fn thunder_packet_gives_vanilla_cloud_colour() {
        let mut w = Weather::default();
        w.apply(7, 1.0);
        w.apply(8, 1.0);
        let (rain, thunder) = w.strengths();
        let env = render::Environment {
            rain,
            thunder,
            ..Default::default()
        };
        let expected = 0.62 * 0.05 + 0.62 * 0.2 * 0.95;
        for c in env.sky_state(0.0, [0.0, 0.0, 1.0]).cloud {
            assert!((c - expected).abs() < 1e-3, "{c}");
        }
    }
}
