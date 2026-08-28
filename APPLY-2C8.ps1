$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Target = Join-Path $Root 'crates\phantom-browser\src\main.rs'

if (-not (Test-Path -LiteralPath $Target)) {
    throw "Phantom browser source not found: $Target"
}

$content = [System.IO.File]::ReadAllText($Target)
$hadCrLf = $content.Contains("`r`n")
$content = $content.Replace("`r`n", "`n")

function Replace-ExactOnce {
    param(
        [string]$Text,
        [string]$Old,
        [string]$New,
        [string]$Label
    )

    $first = $Text.IndexOf($Old, [System.StringComparison]::Ordinal)
    if ($first -lt 0) {
        if ($Text.Contains($New)) {
            return $Text
        }
        throw "2C-8 patch point not found: $Label"
    }

    $second = $Text.IndexOf($Old, $first + $Old.Length, [System.StringComparison]::Ordinal)
    if ($second -ge 0) {
        throw "2C-8 patch point is ambiguous: $Label"
    }

    return $Text.Substring(0, $first) + $New + $Text.Substring($first + $Old.Length)
}

$content = Replace-ExactOnce $content `
    'use phantom_net::{HttpUrl, NetworkClient, NetworkError, TextResponse};' `
    'use phantom_net::{HttpUrl, NetworkClient, NetworkError, NetworkIsolationKey, TextResponse};' `
    'phantom-net import'

$old = @'
struct ImageLoadRequest {
    resources: Vec<ImageResourceId>,
    url: HttpUrl,
    loading: ImageLoading,
    top: f32,
}
'@
$new = @'
struct ImageLoadRequest {
    resources: Vec<ImageResourceId>,
    url: HttpUrl,
    isolation_key: NetworkIsolationKey,
    loading: ImageLoading,
    top: f32,
}
'@
$content = Replace-ExactOnce $content $old $new 'ImageLoadRequest isolation key'

$old = @'
                let result = fetch_and_decode_image(
                    &client,
                    &decoder,
                    limits,
                    animation_limits,
                    &request.url,
                );
'@
$new = @'
                let result = fetch_and_decode_image(
                    &client,
                    &decoder,
                    limits,
                    animation_limits,
                    &request.isolation_key,
                    &request.url,
                );
'@
$content = Replace-ExactOnce $content $old $new 'image worker isolation key'

$old = @'
) -> Vec<ImageLoadRequest> {
    let discovered = tab.engine.image_requests_for_device(device_pixel_ratio);

    let mut grouped = BTreeMap::<String, (HttpUrl, Vec<ImageResourceId>, ImageLoading, f32)>::new();
'@
$new = @'
) -> Vec<ImageLoadRequest> {
    let discovered = tab.engine.image_requests_for_device(device_pixel_ratio);
    let isolation_key = NetworkIsolationKey::from_top_level(base_url);

    let mut grouped = BTreeMap::<String, (HttpUrl, Vec<ImageResourceId>, ImageLoading, f32)>::new();
'@
$content = Replace-ExactOnce $content $old $new 'collect image isolation key'

$old = @'
        requests.push(ImageLoadRequest {
            resources,
            url,
            loading,
            top,
        });
'@
$new = @'
        requests.push(ImageLoadRequest {
            resources,
            url,
            isolation_key: isolation_key.clone(),
            loading,
            top,
        });
'@
$content = Replace-ExactOnce $content $old $new 'ImageLoadRequest construction'

$old = @'
fn fetch_and_decode_image(
    network: &NetworkClient,
    decoder: &RasterImageDecoder,
    limits: ImageDecodeLimits,
    animation_limits: AnimationDecodeLimits,
    url: &HttpUrl,
) -> Result<LoadedImage, String> {
    let response = network
        .fetch_bytes(url)
        .map_err(|error| error.to_string())?;
'@
$new = @'
fn fetch_and_decode_image(
    network: &NetworkClient,
    decoder: &RasterImageDecoder,
    limits: ImageDecodeLimits,
    animation_limits: AnimationDecodeLimits,
    isolation_key: &NetworkIsolationKey,
    url: &HttpUrl,
) -> Result<LoadedImage, String> {
    let response = network
        .fetch_bytes_partitioned(isolation_key, url)
        .map_err(|error| error.to_string())?;
'@
$content = Replace-ExactOnce $content $old $new 'partitioned binary fetch'

if ($hadCrLf) {
    $content = $content.Replace("`n", "`r`n")
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($Target, $content, $utf8NoBom)

Write-Host 'Phantom 2C-8 browser wiring applied successfully.'
