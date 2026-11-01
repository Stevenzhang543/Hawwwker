//! Copilot：技能、会话存储、流式回答与动作解析。

use super::{prompts, AiClient, Msg, Opts};
use crate::db::{now_iso, Db};
use crate::error::AppResult;
use crate::models::{CopilotAction, CopilotDelta, CopilotDone, CopilotMessage, CopilotSendInput, CopilotSkill};
use rusqlite::params;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

fn skill(id: &str, name: &str, icon: &str, description: &str, starter: &str, roles: &[&str]) -> CopilotSkill {
    CopilotSkill {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        description: description.into(),
        starter: starter.into(),
        roles: roles.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn skills() -> Vec<CopilotSkill> {
    vec![
        skill("gift", "礼物构思", "gift", "按对象、场合、预算给出礼物方向，并一键去搜", "帮我给喜欢露营的爸爸挑 500 元以内的生日礼物", &["consumer"]),
        skill("compare", "比价顾问", "scale", "解读当前搜索结果：哪家最划算、要注意什么", "这些结果里哪个最值得买？", &["consumer", "merchant"]),
        skill("timing", "入手时机", "timer", "结合价格走势判断现在买还是再等等", "现在入手合适吗？", &["consumer"]),
        skill("industry", "行业判断", "compass", "某个品类的市场格局、价格带与趋势判断", "帮我判断一下便携咖啡机这个品类值不值得做", &["merchant", "analyst"]),
        skill("selection", "选品分析", "box", "从价格带、竞争和平台差异给出选品建议", "我想在 Amazon 德国站卖露营灯，怎么选品？", &["merchant"]),
        skill("pricing", "定价建议", "tag", "基于同款各平台价格给出定价区间", "我的蓝牙耳机该定价多少？", &["merchant"]),
        skill("copy", "商品文案", "pen", "为商品写多平台标题与卖点文案（合规用语）", "为这款商品写一个小红书种草标题和 3 条卖点", &["merchant"]),
        skill("report", "数据解读", "chart", "解读追踪与监控数据，给出结论", "帮我总结最近 30 天追踪商品的价格变化", &["analyst", "merchant"]),
    ]
}

fn skill_prompt(id: &str) -> &'static str {
    match id {
        "gift" => "当前技能：礼物构思。先给 3~5 个礼物方向（每个方向：适合原因、价格带、具体可搜的商品词），再附上对应的 search 动作。缺少关键信息（对象年龄/喜好/预算）时，先按常见情况给建议，最后用一句话追问。",
        "compare" => "当前技能：比价顾问。基于上下文中的搜索结果比较价格、匹配度、平台与风险，给出明确推荐（引用商品 id 对应的标题）。",
        "timing" => "当前技能：入手时机。结合上下文中的价格历史与常见促销节点（双11、618、黑五、Prime Day 等）判断现在是否值得买，必要时建议 watch 动作设置目标价。",
        "industry" => "当前技能：行业判断。从需求、竞争、价格带、渠道、季节性、风险六个方面给出判断，并建议用 Hawwwker 搜索哪些关键词验证。",
        "selection" => "当前技能：选品分析。给出 3 个候选细分方向及理由、目标价格带与差异化点，并附 search 动作用于验证。",
        "pricing" => "当前技能：定价建议。根据同款或相近商品在各平台的价格分布，给出入门/主流/高端三档定价与理由。",
        "copy" => "当前技能：商品文案。输出：1 个标题、3~5 条卖点、1 段 80 字以内描述；避免《广告法》极限词（最、第一、国家级等），不夸大。",
        "report" => "当前技能：数据解读。基于上下文数据给出 3~5 条结论与建议，数字只引用上下文中给出的。",
        _ => "",
    }
}

/// 拆出回答末尾的 ```actions 代码块。
pub fn split_actions(text: &str) -> (String, Vec<CopilotAction>) {
    let Some(start) = text.find("```actions") else { return (text.trim().to_string(), vec![]) };
    let body_start = start + "```actions".len();
    let end = text[body_start..].find("```").map(|e| body_start + e).unwrap_or(text.len());
    let block = &text[body_start..end];
    let mut actions = vec![];
    if let Some(json) = super::extract_json(block) {
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(json) {
            for a in arr.into_iter().take(4) {
                if let Ok(act) = serde_json::from_value::<CopilotAction>(a) {
                    let valid = match &act {
                        CopilotAction::Open { url, .. } => url.starts_with("https://") || url.starts_with("http://"),
                        CopilotAction::Navigate { route, .. } => route.starts_with('/'),
                        CopilotAction::Search { instruction, .. } => !instruction.trim().is_empty(),
                        CopilotAction::Watch { listing_id, .. } => !listing_id.is_empty(),
                    };
                    if valid {
                        actions.push(act);
                    }
                }
            }
        }
    }
    let mut content = String::new();
    content.push_str(text[..start].trim_end());
    let rest = text.get(end + 3..).unwrap_or("").trim();
    if !rest.is_empty() {
        content.push_str("\n\n");
        content.push_str(rest);
    }
    (content.trim().to_string(), actions)
}

pub fn history(db: &Db, session: &str) -> AppResult<Vec<CopilotMessage>> {
    let c = db.conn();
    let mut stmt = c.prepare(
        "SELECT id, session_id, role, content, actions, created_at FROM copilot_messages WHERE session_id=?1 ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([session], |r| {
        Ok(CopilotMessage {
            id: r.get(0)?,
            session_id: r.get(1)?,
            role: r.get(2)?,
            content: r.get(3)?,
            actions: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
            created_at: r.get(5)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn save(db: &Db, m: &CopilotMessage) -> AppResult<()> {
    db.conn().execute(
        "INSERT INTO copilot_messages(id, session_id, role, content, actions, created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![m.id, m.session_id, m.role, m.content, serde_json::to_string(&m.actions)?, m.created_at],
    )?;
    Ok(())
}

pub fn reset(db: &Db, session: &str) -> AppResult<()> {
    db.conn().execute("DELETE FROM copilot_messages WHERE session_id=?1", [session])?;
    Ok(())
}

/// 保存用户消息并在后台流式生成回答；立即返回回答消息 id。
pub fn send(app: AppHandle, db: Db, ai: Arc<AiClient>, role: String, input: CopilotSendInput) -> AppResult<String> {
    ai.require()?;
    let text = input.text.trim().to_string();
    if text.is_empty() {
        return Err(crate::error::AppError::invalid("请输入问题"));
    }
    let prior = history(&db, &input.session_id)?;
    let user_msg = CopilotMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: input.session_id.clone(),
        role: "user".into(),
        content: text.chars().take(4000).collect(),
        actions: vec![],
        created_at: now_iso(),
    };
    save(&db, &user_msg)?;
    let reply_id = uuid::Uuid::new_v4().to_string();
    let rid = reply_id.clone();
    tauri::async_runtime::spawn(async move {
        let mut msgs = vec![Msg::system(prompts::COPILOT), Msg::system(format!("用户角色：{role}。今天日期：{}。", chrono::Local::now().format("%Y-%m-%d")))];
        if let Some(skill) = input.skill.as_deref() {
            let sp = skill_prompt(skill);
            if !sp.is_empty() {
                msgs.push(Msg::system(sp));
            }
        }
        if let Some(ctx) = input.context.as_ref().filter(|c| !c.is_null()) {
            let mut raw = ctx.to_string();
            if raw.len() > 12_000 {
                raw = raw.chars().take(12_000).collect();
            }
            msgs.push(Msg::system(format!("当前页面上下文（JSON，商品 id 可用于 watch 动作）：\n{raw}")));
        }
        for m in prior.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
            msgs.push(if m.role == "user" { Msg::user(m.content.clone()) } else { Msg::assistant(m.content.clone()) });
        }
        msgs.push(Msg::user(text));
        let sid = input.session_id.clone();
        let emit_app = app.clone();
        let res = ai
            .stream(&msgs, Opts { max_tokens: Some(2048), temperature: Some(0.5), ..Default::default() }, |delta| {
                let _ = emit_app.emit("copilot://delta", CopilotDelta { session_id: sid.clone(), message_id: rid.clone(), delta: delta.to_string() });
            })
            .await;
        let (content, actions) = match res {
            Ok(full) => split_actions(&full),
            Err(e) => (format!("⚠️ 暂时无法回答：{}", e.message), vec![]),
        };
        let msg = CopilotMessage {
            id: rid.clone(),
            session_id: input.session_id.clone(),
            role: "assistant".into(),
            content,
            actions,
            created_at: now_iso(),
        };
        let _ = save(&db, &msg);
        let _ = app.emit("copilot://done", CopilotDone { session_id: input.session_id, message: msg });
    });
    Ok(reply_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_block_parsed_and_removed() {
        let text = "推荐三个方向：\n1. 露营灯\n```actions\n[{\"type\":\"search\",\"label\":\"搜露营灯\",\"instruction\":\"露营灯 300元以内\"},{\"type\":\"open\",\"label\":\"x\",\"url\":\"javascript:alert(1)\"}]\n```";
        let (content, actions) = split_actions(text);
        assert_eq!(content, "推荐三个方向：\n1. 露营灯");
        assert_eq!(actions.len(), 1, "unsafe url dropped");
        let (c2, a2) = split_actions("没有动作");
        assert_eq!((c2.as_str(), a2.len()), ("没有动作", 0));
    }
}
