//! 快递查询:单号 → 物流轨迹。
//!
//! **没有 key 时不假装查到了。** 快递 100 的实时查询要 `customer` + `key`;
//! 没配的话这件工具返回 `ok:true, state:"no_credentials"`,带上单号、识别出的
//! 快递公司和官方查询入口,并在 `note` 里写清缺什么。这是刻意的:一个语言模型
//! 手里拿着单号,最容易发生的事就是**编一条看起来很像的物流轨迹**——「已到达
//! 上海转运中心」这种句子它张口就来,而用户会当真。查不到要说查不到。
//!
//! 识别快递公司走快递 100 的号码识别口:配了 key 走文档里的 `auto`,没配走免费的
//! `autoComNum`。两个口的返回形状不同(裸数组 vs 信封),`first_com_code` 两种都认。
//! 给的是候选列表,第一条是最可能的那家;用户显式传了 `company` 就不识别。
//!
//! 顺丰查询要收件人手机后四位,这是顺丰自己的规矩,不是这里加的——缺了会被
//! 上游拒,所以 `phone` 缺失时提前说清楚,别让用户对着一句 `sign error` 猜。

use super::{http_response, ToolRegistry, ToolSpec};
use crate::config::ExpressPluginConfig;
use anyhow::{bail, Result};
use md5::{Digest, Md5};
use serde_json::{json, Value};

const AUTO_NUMBER_URL: &str = "https://www.kuaidi100.com/autonumber/autoComNum";
const AUTO_KEYED_URL: &str = "https://www.kuaidi100.com/autonumber/auto";
const POLL_URL: &str = "https://poll.kuaidi100.com/poll/query.do";

pub fn register(registry: &mut ToolRegistry, config: ExpressPluginConfig) {
    registry.register(
        ToolSpec::new(
            "express_query",
            "占位描述(注册时被 descriptions/express.json 整体覆盖)",
            json!({
                "type": "object",
                "properties": {
                    "number": { "type": "string", "description": "The tracking number." },
                    "company": { "type": "string", "description": "Optional carrier code such as shunfeng, yuantong, zhongtong, jd. Detected from the number when omitted." },
                    "phone": { "type": "string", "description": "Recipient phone, or its last four digits. SF Express refuses to answer without it." }
                },
                "required": ["number"],
                "additionalProperties": false
            }),
            move |args| {
                let config = config.clone();
                async move { express_query(args, config).await }
            },
        )
        .with_groups(vec!["web".to_string()])
        .with_stub_example(r#"{"number":"SF1234567890"}"#),
    );
}

/// 单号里只该有字母数字和连字符。这里挡一道,免得把用户随手粘的一整句话
/// 当单号发出去。
fn clean_number(raw: &str) -> Result<String> {
    let number: String = raw
        .trim()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if number.len() < 6 || number.len() > 40 {
        bail!("that does not look like a tracking number");
    }
    if !number
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        bail!("a tracking number is letters, digits and dashes only");
    }
    Ok(number)
}

async fn express_query(args: Value, config: ExpressPluginConfig) -> Result<String> {
    let number = clean_number(
        args.get("number")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let phone = args
        .get("phone")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let asked_company = args
        .get("company")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();

    let customer = config.kuaidi100_customer.trim().to_string();
    let key = config.kuaidi100_key.trim().to_string();
    let company = if asked_company.is_empty() {
        detect_company(&number, &key).await.unwrap_or_default()
    } else {
        normalize_company(&asked_company)
    };

    if customer.is_empty() || key.is_empty() {
        return Ok(serde_json::to_string(&json!({
            "ok": true,
            "state": "no_credentials",
            "number": number,
            "company": company,
            "company_name": company_name(&company),
            "traces": [],
            "official_url": official_url(&number),
            "note": "no kuaidi100 credentials configured (plugins.express.kuaidi100_customer / kuaidi100_key), so the live trace is not available. Tell the user exactly this and offer the official link — do NOT invent any shipment status.",
        }))?);
    }
    if company.is_empty() {
        return Ok(serde_json::to_string(&json!({
            "ok": true,
            "state": "unknown_company",
            "number": number,
            "traces": [],
            "official_url": official_url(&number),
            "note": "could not tell which carrier this number belongs to; ask the user and call again with company=",
        }))?);
    }
    if company == "shunfeng" && phone.is_empty() {
        return Ok(serde_json::to_string(&json!({
            "ok": true,
            "state": "needs_phone",
            "number": number,
            "company": company,
            "company_name": company_name(&company),
            "traces": [],
            "official_url": official_url(&number),
            "note": "SF Express requires the recipient's phone (last four digits are enough); ask the user and call again with phone=",
        }))?);
    }

    let param = json!({
        "com": company,
        "num": number,
        "phone": phone,
        "resultv2": "4",
        "show": "0",
        "order": "desc",
    })
    .to_string();
    let sign = sign_of(&param, &key, &customer);
    let form = [
        ("customer", customer.as_str()),
        ("sign", sign.as_str()),
        ("param", param.as_str()),
    ];
    let data: Value = http_response::shared_client()
        .post(POLL_URL)
        .form(&form)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // 快递 100 的业务错误也是 200:有 `returnCode` 且不是 200 就是失败。
    let return_code = data
        .get("returnCode")
        .and_then(Value::as_str)
        .unwrap_or("200");
    if return_code != "200" {
        let message = data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("query failed");
        return Ok(serde_json::to_string(&json!({
            "ok": false,
            "state": "provider_error",
            "number": number,
            "company": company,
            "company_name": company_name(&company),
            "traces": [],
            "official_url": official_url(&number),
            "note": format!("kuaidi100 said: {message} (code {return_code})"),
        }))?);
    }

    let traces: Vec<Value> = data
        .get("data")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    json!({
                        "time": item.get("ftime").and_then(Value::as_str).or_else(|| item.get("time").and_then(Value::as_str)).unwrap_or_default(),
                        "text": item.get("context").and_then(Value::as_str).unwrap_or_default(),
                        "location": item.get("areaName").and_then(Value::as_str).unwrap_or_default(),
                        "status": item.get("status").and_then(Value::as_str).unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let state_code = data
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    Ok(serde_json::to_string(&json!({
        "ok": true,
        "state": "ok",
        "number": number,
        "company": company,
        "company_name": company_name(&company),
        "status": state_label(state_code),
        "status_code": state_code,
        "is_check": data.get("ischeck").and_then(Value::as_str) == Some("1"),
        "traces": traces,
        "official_url": official_url(&number),
    }))?)
}

/// `MD5(param + key + customer)`,大写十六进制。快递 100 定的规矩。
fn sign_of(param: &str, key: &str, customer: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(param.as_bytes());
    hasher.update(key.as_bytes());
    hasher.update(customer.as_bytes());
    format!("{:X}", hasher.finalize())
}

/// 号码识别。配了 key 走文档里的 `auto` 口,没配走免费的 `autoComNum`。
/// 识别不出来就是空,交由上层去问用户——猜一家快递公司去查,查出来的是
/// 别人的包裹。
async fn detect_company(number: &str, key: &str) -> Option<String> {
    let url = if key.is_empty() {
        format!("{AUTO_NUMBER_URL}?text={}", urlencoding::encode(number))
    } else {
        format!(
            "{AUTO_KEYED_URL}?num={}&key={}",
            urlencoding::encode(number),
            urlencoding::encode(key)
        )
    };
    let data: Value = http_response::shared_client()
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    first_com_code(&data)
}

/// 两个识别口的返回**形状不同**:`auto` 给一个裸数组,`autoComNum` 给一个
/// `{returnCode, message, data:[…]}` 的信封(单号不合法时 returnCode=201、
/// 压根没有 data)。只认其中一种,换一条路就静默识别不出来。
fn first_com_code(data: &Value) -> Option<String> {
    let items = match data {
        Value::Array(items) => items.clone(),
        Value::Object(_) => {
            if let Some(code) = data.get("returnCode").and_then(Value::as_str) {
                if code != "200" {
                    return None;
                }
            }
            data.get("data").and_then(Value::as_array).cloned()?
        }
        _ => return None,
    };
    let code = items
        .first()?
        .get("comCode")
        .and_then(Value::as_str)?
        .trim();
    (!code.is_empty()).then(|| code.to_string())
}

/// 用户会写「顺丰」「SF」「sf-express」,上游只认它自己那套小写拼音码。
fn normalize_company(raw: &str) -> String {
    let lowered = raw.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "顺丰" | "顺丰速运" | "sf" | "sf-express" | "sfexpress" => "shunfeng",
        "圆通" | "yto" => "yuantong",
        "中通" | "zto" => "zhongtong",
        "申通" | "sto" => "shentong",
        "韵达" | "yunda" | "yd" => "yunda",
        "京东" | "jd" | "京东物流" => "jd",
        "邮政" | "中国邮政" | "ems" => "ems",
        "极兔" | "jitu" | "j&t" | "jt" => "jtexpress",
        "德邦" | "deppon" => "debangkuaidi",
        "菜鸟" | "cainiao" => "cainiao",
        other => return other.to_string(),
    }
    .to_string()
}

fn company_name(code: &str) -> String {
    match code {
        "shunfeng" => "顺丰速运",
        "yuantong" => "圆通速递",
        "zhongtong" => "中通快递",
        "shentong" => "申通快递",
        "yunda" => "韵达速递",
        "jd" => "京东物流",
        "ems" => "中国邮政",
        "youzhengguonei" => "邮政快递包裹",
        "jtexpress" => "极兔速递",
        "debangkuaidi" => "德邦快递",
        "cainiao" => "菜鸟速递",
        other => other,
    }
    .to_string()
}

/// 快递 100 的状态码。给模型看的是中文标签,`status_code` 原样留着。
fn state_label(code: &str) -> String {
    match code {
        "0" => "在途",
        "1" => "已揽收",
        "2" => "疑难",
        "3" => "已签收",
        "4" => "退签",
        "5" => "派件中",
        "6" => "退回",
        "7" => "转投",
        "8" => "清关",
        "14" => "拒签",
        _ => "未知",
    }
    .to_string()
}

fn official_url(number: &str) -> String {
    format!(
        "https://www.kuaidi100.com/chaxun?nu={}",
        urlencoding::encode(number)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_the_known_vectors() {
        // sign 算错的表现是上游一句 `sign error`,查不出是自己算错还是 key 配错。
        // 这两条向量把前一半钉死。
        let mut hasher = Md5::new();
        hasher.update(b"abc");
        assert_eq!(
            format!("{:x}", hasher.finalize()),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        // 拼接顺序是 param + key + customer,大写。
        assert_eq!(sign_of("a", "b", "c"), sign_of("ab", "", "c"));
        assert!(sign_of("a", "b", "c").chars().all(|ch| !ch.is_lowercase()));
    }

    #[test]
    fn a_pasted_sentence_is_not_a_tracking_number() {
        assert!(clean_number("帮我查一下这个单号好吗").is_err());
        assert!(clean_number("SF").is_err());
        assert_eq!(clean_number(" SF1234 567890 ").unwrap(), "SF1234567890");
    }

    #[test]
    fn both_detection_response_shapes_are_understood() {
        // `auto`(带 key)给裸数组;`autoComNum`(免费)给一个信封。
        assert_eq!(
            first_com_code(&json!([{ "comCode": "yuantong" }])).as_deref(),
            Some("yuantong")
        );
        assert_eq!(
            first_com_code(&json!({ "returnCode": "200", "data": [{ "comCode": "jd" }] }))
                .as_deref(),
            Some("jd")
        );
        // 单号不合法时免费口给 201 且没有 data——当成「认不出」,不是崩。
        assert_eq!(
            first_com_code(
                &json!({ "returnCode": "201", "message": "不是有效的快递单号", "result": false })
            ),
            None
        );
    }

    #[test]
    fn carrier_aliases_land_on_the_upstream_codes() {
        assert_eq!(normalize_company("顺丰"), "shunfeng");
        assert_eq!(normalize_company("SF"), "shunfeng");
        assert_eq!(normalize_company("zto"), "zhongtong");
        // 认不出的原样透传:上游认得的码比这张表多。
        assert_eq!(normalize_company("suer"), "suer");
    }
}
