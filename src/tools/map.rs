//! 地图:地址/POI → 坐标,坐标 → 地址。
//!
//! **为什么要有这件工具。** 以前她回答「XX 在哪」只能贴一条高德网址,WebUI 把它
//! 升级成链接卡片——那张卡上没有地图,点进去才看得到,等于什么都没解决。真正缺
//! 的是**坐标**:有了坐标,前端才画得出带标记点的地图(`web/mapcard.js` 那张卡,
//! 瓦片经 `/api/map/tile` 代理回本机,所以 CSP 的 `img-src 'self'` 一个字都不用放宽)。
//!
//! **两条数据源,不是二选一。**
//!
//! | | 开源(默认) | 高德 |
//! |---|---|---|
//! | 检索 | Nominatim | restapi.amap.com |
//! | 瓦片 | OSM 官方瓦片 | 高德栅格瓦片 |
//! | key | 不要 | 要 Web 服务 key |
//! | 坐标系 | WGS-84 | GCJ-02 |
//! | 国内覆盖 | 一般,门牌与 POI 稀 | 好 |
//!
//! `provider = "auto"`(默认)时:配了 `amap_key` 走高德,没配走 OSM。显式写
//! `osm` / `amap` 就不再自动。
//!
//! **坐标系这件事不能含糊。** 高德给的是 GCJ-02(国测局偏移),OSM 给的是 WGS-84,
//! 两者在国内差几百米——把一个datum 的点画到另一个datum 的瓦片上,标记就会落在
//! 隔壁街。所以每个结果**两套坐标都带**(`lon`/`lat` 是该源的原生 datum,
//! `lon_wgs84` / `lon_gcj02` 是换算过的),卡片按 provider 选瓦片与对应那套坐标,
//! 外链也各用各的:高德链接要 GCJ-02,OSM 链接要 WGS-84。

use super::{http_response, ToolRegistry, ToolSpec};
use crate::config::MapPluginConfig;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// Nominatim 的使用条款要求带一个能联系上的 User-Agent。
const OSM_USER_AGENT: &str = "Miyu/0.5 (personal assistant; https://github.com/yxxbc/Miyu)";

pub fn register(registry: &mut ToolRegistry, config: MapPluginConfig) {
    registry.register(
        ToolSpec::new(
            "map_search",
            "占位描述(注册时被 descriptions/map.json 整体覆盖)",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for: an address, a place name or a category such as 咖啡馆." },
                    "city": { "type": "string", "description": "Optional city to search in, e.g. 上海." },
                    "around": { "type": "string", "description": "Optional 'lon,lat' to search near." },
                    "reverse": { "type": "string", "description": "A 'lon,lat' to turn into an address. Use instead of query." },
                    "limit": { "type": "integer", "description": "Max results, 1-10. Default 5." }
                },
                "additionalProperties": false
            }),
            move |args| {
                let config = config.clone();
                async move { map_search(args, config).await }
            },
        )
        .with_groups(vec!["web".to_string()])
        .with_stub_example(r#"{"query":"上海图书馆东馆"}"#),
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Provider {
    Osm,
    Amap,
}

impl Provider {
    fn key(self) -> &'static str {
        match self {
            Provider::Osm => "osm",
            Provider::Amap => "amap",
        }
    }
    /// 这个源原生给的坐标是哪套。
    fn datum(self) -> &'static str {
        match self {
            Provider::Osm => "wgs84",
            Provider::Amap => "gcj02",
        }
    }
}

fn pick_provider(config: &MapPluginConfig) -> Provider {
    match config.provider.trim().to_ascii_lowercase().as_str() {
        "amap" => Provider::Amap,
        "osm" | "openstreetmap" | "nominatim" => Provider::Osm,
        // auto:有 key 才谈得上高德。
        _ if !config.amap_key.trim().is_empty() => Provider::Amap,
        _ => Provider::Osm,
    }
}

async fn map_search(args: Value, config: MapPluginConfig) -> Result<String> {
    let provider = pick_provider(&config);
    if provider == Provider::Amap && config.amap_key.trim().is_empty() {
        bail!("map provider is amap but plugins.map.amap_key is empty; set it or switch provider to osm");
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(1, 10) as usize;
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let city = args
        .get("city")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let reverse = args
        .get("reverse")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let around = args
        .get("around")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();

    let places = if !reverse.is_empty() {
        let (lon, lat) = parse_lon_lat(reverse)?;
        match provider {
            Provider::Amap => amap_reverse(&config, lon, lat).await?,
            Provider::Osm => osm_reverse(&config, lon, lat).await?,
        }
    } else {
        if query.is_empty() {
            bail!("give a query (an address or place name), or a reverse='lon,lat'");
        }
        let center = if around.is_empty() {
            None
        } else {
            Some(parse_lon_lat(around)?)
        };
        match provider {
            Provider::Amap => amap_search(&config, query, city, center, limit).await?,
            Provider::Osm => osm_search(&config, query, city, center, limit).await?,
        }
    };

    if places.is_empty() {
        // 空结果不是错误:让模型照直说「没找到」,而不是把一次成功的查询报成失败。
        return Ok(serde_json::to_string(&json!({
            "ok": true,
            "provider": provider.key(),
            "count": 0,
            "places": [],
            "note": "nothing matched; try a fuller address, or add a city",
        }))?);
    }

    let places: Vec<Value> = places
        .into_iter()
        .take(limit)
        .map(|place| place.into_json(provider))
        .collect();
    Ok(serde_json::to_string(&json!({
        "ok": true,
        "provider": provider.key(),
        "datum": provider.datum(),
        "query": if reverse.is_empty() { query } else { reverse },
        "count": places.len(),
        "places": places,
    }))?)
}

/// 一个地点。`lon`/`lat` 永远是**该源原生 datum** 的坐标,换算在 `into_json`。
struct Place {
    name: String,
    address: String,
    lon: f64,
    lat: f64,
    category: String,
    /// 距 `around` 的直线距离(米),源给了才有。
    distance_m: Option<f64>,
}

impl Place {
    fn into_json(self, provider: Provider) -> Value {
        let (wgs, gcj) = match provider {
            Provider::Osm => ((self.lon, self.lat), wgs84_to_gcj02(self.lon, self.lat)),
            Provider::Amap => (gcj02_to_wgs84(self.lon, self.lat), (self.lon, self.lat)),
        };
        json!({
            "name": self.name,
            "address": self.address,
            "category": self.category,
            "lon": round6(self.lon),
            "lat": round6(self.lat),
            "lon_wgs84": round6(wgs.0),
            "lat_wgs84": round6(wgs.1),
            "lon_gcj02": round6(gcj.0),
            "lat_gcj02": round6(gcj.1),
            "distance_m": self.distance_m.map(|meters| meters.round()),
            // 外链各用各的 datum,搞反了就是「点开一看在隔壁街」。
            "amap_url": format!(
                "https://uri.amap.com/marker?position={},{}&name={}",
                round6(gcj.0),
                round6(gcj.1),
                urlencoding::encode(&self.name)
            ),
            "osm_url": format!(
                "https://www.openstreetmap.org/?mlat={}&mlon={}#map=17/{}/{}",
                round6(wgs.1),
                round6(wgs.0),
                round6(wgs.1),
                round6(wgs.0)
            ),
        })
    }
}

fn round6(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

/// `"lon,lat"` → (lon, lat)。两个数都要在合法范围内——把 lat 写在前面是最常见的
/// 手滑,值域检查能当场抓住大部分。
fn parse_lon_lat(raw: &str) -> Result<(f64, f64)> {
    let (lon, lat) = raw
        .split_once(',')
        .context("expected 'lon,lat', e.g. 121.47,31.23")?;
    let lon: f64 = lon.trim().parse().context("longitude is not a number")?;
    let lat: f64 = lat.trim().parse().context("latitude is not a number")?;
    if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
        bail!("coordinates out of range; the order is lon,lat");
    }
    Ok((lon, lat))
}

/* ── OpenStreetMap / Nominatim ─────────────────────────────────────── */

fn nominatim_base(config: &MapPluginConfig) -> String {
    let base = config.nominatim_base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        "https://nominatim.openstreetmap.org".to_string()
    } else {
        base.to_string()
    }
}

async fn osm_get(url: String) -> Result<Value> {
    let response = http_response::shared_client()
        .get(&url)
        .header(reqwest::header::USER_AGENT, OSM_USER_AGENT)
        .header(reqwest::header::ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json().await?)
}

async fn osm_search(
    config: &MapPluginConfig,
    query: &str,
    city: &str,
    center: Option<(f64, f64)>,
    limit: usize,
) -> Result<Vec<Place>> {
    let text = if city.is_empty() {
        query.to_string()
    } else {
        format!("{query} {city}")
    };
    let mut url = format!(
        "{}/search?q={}&format=jsonv2&addressdetails=1&limit={limit}&accept-language=zh-CN",
        nominatim_base(config),
        urlencoding::encode(&text)
    );
    // Nominatim 没有「以点为心、按半径」的检索,只有取景框加权。用 ±0.05°
    // (约 5km)围一个框并 bounded=0——框外的结果仍然给,只是排在后面。
    if let Some((lon, lat)) = center {
        url.push_str(&format!(
            "&viewbox={},{},{},{}&bounded=0",
            lon - 0.05,
            lat + 0.05,
            lon + 0.05,
            lat - 0.05
        ));
    }
    let data = osm_get(url).await?;
    let items = data.as_array().cloned().unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|item| osm_place(item, center))
        .collect())
}

async fn osm_reverse(config: &MapPluginConfig, lon: f64, lat: f64) -> Result<Vec<Place>> {
    let url = format!(
        "{}/reverse?lon={lon}&lat={lat}&format=jsonv2&addressdetails=1&accept-language=zh-CN",
        nominatim_base(config)
    );
    let data = osm_get(url).await?;
    Ok(osm_place(&data, None).into_iter().collect())
}

fn osm_place(item: &Value, center: Option<(f64, f64)>) -> Option<Place> {
    let lon: f64 = item.get("lon")?.as_str()?.parse().ok()?;
    let lat: f64 = item.get("lat")?.as_str()?.parse().ok()?;
    let address = item
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // Nominatim 的 `name` 常是空的(纯地址结果),退回 display_name 的第一段。
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            address
                .split(',')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        });
    let category = match (
        item.get("category").and_then(Value::as_str),
        item.get("type").and_then(Value::as_str),
    ) {
        (Some(category), Some(kind)) => format!("{category}/{kind}"),
        (Some(category), None) => category.to_string(),
        (None, Some(kind)) => kind.to_string(),
        _ => String::new(),
    };
    Some(Place {
        name,
        address,
        lon,
        lat,
        category,
        distance_m: center.map(|(clon, clat)| haversine_m(clon, clat, lon, lat)),
    })
}

/* ── 高德 ──────────────────────────────────────────────────────────── */

async fn amap_get(url: String) -> Result<Value> {
    let data: Value = http_response::shared_client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    // 高德把业务错误放在 200 响应里:status=0 才是失败,info 是原因。
    if data.get("status").and_then(Value::as_str) == Some("0") {
        let info = data
            .get("info")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        bail!("amap: {info}");
    }
    Ok(data)
}

async fn amap_search(
    config: &MapPluginConfig,
    query: &str,
    city: &str,
    center: Option<(f64, f64)>,
    limit: usize,
) -> Result<Vec<Place>> {
    let key = config.amap_key.trim();
    let url = match center {
        Some((lon, lat)) => format!(
            "https://restapi.amap.com/v3/place/around?key={key}&location={lon},{lat}&keywords={}&offset={limit}&page=1&extensions=base",
            urlencoding::encode(query)
        ),
        None => format!(
            "https://restapi.amap.com/v3/place/text?key={key}&keywords={}&city={}&citylimit={}&offset={limit}&page=1&extensions=base",
            urlencoding::encode(query),
            urlencoding::encode(city),
            if city.is_empty() { "false" } else { "true" }
        ),
    };
    let data = amap_get(url).await?;
    let pois = data
        .get("pois")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut places: Vec<Place> = pois.iter().filter_map(amap_poi).collect();
    // POI 一条没有时退回地理编码:「某某路 123 号」这种门牌地址在 POI 库里
    // 常常不是一个点,但地理编码认得。
    if places.is_empty() && center.is_none() {
        places = amap_geocode(config, query, city).await?;
    }
    Ok(places)
}

async fn amap_geocode(config: &MapPluginConfig, query: &str, city: &str) -> Result<Vec<Place>> {
    let url = format!(
        "https://restapi.amap.com/v3/geocode/geo?key={}&address={}&city={}",
        config.amap_key.trim(),
        urlencoding::encode(query),
        urlencoding::encode(city)
    );
    let data = amap_get(url).await?;
    let items = data
        .get("geocodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|item| {
            let (lon, lat) = parse_lon_lat(item.get("location")?.as_str()?).ok()?;
            let address = amap_text(item.get("formatted_address"));
            Some(Place {
                name: if address.is_empty() {
                    query.to_string()
                } else {
                    address.clone()
                },
                address,
                lon,
                lat,
                category: amap_text(item.get("level")),
                distance_m: None,
            })
        })
        .collect())
}

async fn amap_reverse(config: &MapPluginConfig, lon: f64, lat: f64) -> Result<Vec<Place>> {
    let url = format!(
        "https://restapi.amap.com/v3/geocode/regeo?key={}&location={lon},{lat}&extensions=base",
        config.amap_key.trim()
    );
    let data = amap_get(url).await?;
    let Some(component) = data.get("regeocode") else {
        return Ok(Vec::new());
    };
    let address = amap_text(component.get("formatted_address"));
    if address.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Place {
        name: address.clone(),
        address,
        lon,
        lat,
        category: String::new(),
        distance_m: None,
    }])
}

fn amap_poi(item: &Value) -> Option<Place> {
    let (lon, lat) = parse_lon_lat(item.get("location")?.as_str()?).ok()?;
    Some(Place {
        name: amap_text(item.get("name")),
        address: amap_text(item.get("address")),
        lon,
        lat,
        category: amap_text(item.get("type")),
        distance_m: amap_text(item.get("distance")).parse().ok(),
    })
}

/// 高德的可选字段在「没有值」时给的是**空数组** `[]`,不是空字符串——直接
/// `as_str()` 会得到 None 并被当成缺字段,而 `to_string()` 会打印成 `[]`。
fn amap_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

/* ── 坐标系换算 ────────────────────────────────────────────────────── */

const GCJ_A: f64 = 6_378_245.0;
const GCJ_EE: f64 = 0.006_693_421_622_965_943;

/// 国境外不偏移(GCJ-02 只在国内生效)。这个矩形框是粗判,与业界通用实现一致:
/// 边界上判错的代价是几百米的偏差,而更精确的国界判定要一份边界数据。
fn out_of_china(lon: f64, lat: f64) -> bool {
    !(72.004..=137.8347).contains(&lon) || !(0.8293..=55.8271).contains(&lat)
}

fn transform_lat(x: f64, y: f64) -> f64 {
    let mut ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * std::f64::consts::PI).sin() + 20.0 * (2.0 * x * std::f64::consts::PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (y * std::f64::consts::PI).sin() + 40.0 * (y / 3.0 * std::f64::consts::PI).sin()) * 2.0 / 3.0;
    ret += (160.0 * (y / 12.0 * std::f64::consts::PI).sin() + 320.0 * (y * std::f64::consts::PI / 30.0).sin()) * 2.0 / 3.0;
    ret
}

fn transform_lon(x: f64, y: f64) -> f64 {
    let mut ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * std::f64::consts::PI).sin() + 20.0 * (2.0 * x * std::f64::consts::PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (x * std::f64::consts::PI).sin() + 40.0 * (x / 3.0 * std::f64::consts::PI).sin()) * 2.0 / 3.0;
    ret += (150.0 * (x / 12.0 * std::f64::consts::PI).sin() + 300.0 * (x / 30.0 * std::f64::consts::PI).sin()) * 2.0 / 3.0;
    ret
}

fn offset(lon: f64, lat: f64) -> (f64, f64) {
    let d_lat = transform_lat(lon - 105.0, lat - 35.0);
    let d_lon = transform_lon(lon - 105.0, lat - 35.0);
    let rad_lat = lat / 180.0 * std::f64::consts::PI;
    let magic = 1.0 - GCJ_EE * rad_lat.sin() * rad_lat.sin();
    let sqrt_magic = magic.sqrt();
    (
        (d_lon * 180.0) / (GCJ_A / sqrt_magic * rad_lat.cos() * std::f64::consts::PI),
        (d_lat * 180.0) / ((GCJ_A * (1.0 - GCJ_EE)) / (magic * sqrt_magic) * std::f64::consts::PI),
    )
}

pub(crate) fn wgs84_to_gcj02(lon: f64, lat: f64) -> (f64, f64) {
    if out_of_china(lon, lat) {
        return (lon, lat);
    }
    let (d_lon, d_lat) = offset(lon, lat);
    (lon + d_lon, lat + d_lat)
}

/// 反向没有闭式解,迭代逼近。三次就收敛到厘米级,够画地图了。
pub(crate) fn gcj02_to_wgs84(lon: f64, lat: f64) -> (f64, f64) {
    if out_of_china(lon, lat) {
        return (lon, lat);
    }
    let (mut wlon, mut wlat) = (lon, lat);
    for _ in 0..3 {
        let (glon, glat) = wgs84_to_gcj02(wlon, wlat);
        wlon += lon - glon;
        wlat += lat - glat;
    }
    (wlon, wlat)
}

fn haversine_m(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let to_rad = |degrees: f64| degrees * std::f64::consts::PI / 180.0;
    let (p1, p2) = (to_rad(lat1), to_rad(lat2));
    let d_lat = to_rad(lat2 - lat1);
    let d_lon = to_rad(lon2 - lon1);
    let a = (d_lat / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (d_lon / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcj_round_trip_lands_back_within_a_meter() {
        // 天安门(WGS-84)。偏移了几百米,再换回来要落回原地——这条来回如果
        // 漂了,地图上的标记就会落在隔壁街。
        let (lon, lat) = (116.397428, 39.90923);
        let (glon, glat) = wgs84_to_gcj02(lon, lat);
        assert!(haversine_m(lon, lat, glon, glat) > 100.0, "国内应当有偏移");
        let (blon, blat) = gcj02_to_wgs84(glon, glat);
        assert!(haversine_m(lon, lat, blon, blat) < 1.0, "来回应当回到原点");
    }

    #[test]
    fn coordinates_outside_china_are_left_alone() {
        // 东京:GCJ-02 在国境外不偏移,擅自偏一下就是凭空几百米的错。
        let (lon, lat) = (139.767, 35.681);
        assert_eq!(wgs84_to_gcj02(lon, lat), (lon, lat));
        assert_eq!(gcj02_to_wgs84(lon, lat), (lon, lat));
    }

    #[test]
    fn lon_lat_order_is_enforced() {
        assert!(parse_lon_lat("121.47,31.23").is_ok());
        // 纬度写到了经度位上、且超出 90:值域检查抓得住这类手滑。
        assert!(parse_lon_lat("31.23,121.47").is_err());
        assert!(parse_lon_lat("121.47").is_err());
    }

    #[test]
    fn provider_falls_back_to_open_data_without_a_key() {
        let mut config = MapPluginConfig::default();
        assert_eq!(pick_provider(&config).key(), "osm");
        config.amap_key = "k".into();
        assert_eq!(pick_provider(&config).key(), "amap");
        // 显式点名就不再自动。
        config.provider = "osm".into();
        assert_eq!(pick_provider(&config).key(), "osm");
    }

    #[test]
    fn amap_empty_fields_do_not_leak_json_brackets() {
        // 高德给的空值是 `[]`。照直 to_string 会把「[]」写进地址栏。
        let item = json!({ "name": "书店", "address": [] });
        assert_eq!(amap_text(item.get("name")), "书店");
        assert_eq!(amap_text(item.get("address")), "");
    }
}
