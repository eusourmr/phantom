# Standards Map — Animated Images

## Web Platform

Primary HTML reference:

- WHATWG HTML — `img`, `picture`, image fetching and current request/current
  source concepts.

Primary responsive-image behavior remains owned by the 2C-3 candidate-selection
slice.

## Container specifications

GIF behavior is interpreted through GIF89a container timing/repetition metadata.

WebP animation is interpreted through the WebP container animation chunks and
loop metadata.

Phantom delegates binary codec details to the isolated image codec boundary but
converts output into Phantom-owned frame/timing types.

## WPT discipline

This milestone does not claim complete upstream WPT coverage for animated
images. Browser-native tests verify the engine boundary and deterministic tiny
fixtures first.

As the headless/reftest harness matures, unmodified relevant WPT files should be
pinned and executed. Phantom-native tests must not be reported as upstream WPT
passes.

## Non-standard safety policy

The following are implementation/resource policies rather than Web standards:

- maximum 256 retained frames per animation;
- maximum 128 MiB aggregate decoded bytes per animation;
- maximum 256 MiB raster cache per tab;
- 10 ms minimum renderer scheduling interval for a decoded frame.

They must remain clearly separated from HTML/CSS semantics.
