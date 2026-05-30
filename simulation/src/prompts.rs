//! Phase-3 LLM 内省プロンプト (「孤立の恐怖」の内省) とその応答パース．
//!
//! プロンプトはキャッシュキー (`hash(prompt + model)`) の素材になるため，同一状態
//! からは同一プロンプト = 同一応答 (擬似決定論) になるよう決定論的に構築する．
//! `prompts.rs` 自体は socsim-llm に依存しない (純粋な文字列処理) ため，feature
//! `llm` の有無に関わらずコンパイル・テストできる．

/// 私的意見 `b_i` を人間可読なスタンス語へ変換する．
pub fn stance_word(b: f64) -> &'static str {
    if b >= 0.5 {
        "strongly in favour"
    } else if b > 0.0 {
        "mildly in favour"
    } else if b > -0.5 {
        "mildly opposed"
    } else {
        "strongly opposed"
    }
}

/// 知覚多数度を可読語へ．
fn climate_word(pi: f64) -> &'static str {
    if pi >= 0.66 {
        "the clear majority"
    } else if pi >= 0.45 {
        "roughly half"
    } else if pi >= 0.25 {
        "a shrinking minority"
    } else {
        "a small, fading minority"
    }
}

/// 発言意欲を内省させるプロンプト (応答末尾で SPEAK / SILENT を促す)．
///
/// `b`: 私的意見，`pi_now`/`pi_fut`: 現在 / 未来の自意見側多数度知覚，`fear`: 孤立
/// 恐怖，`media`: 媒体シグナル，`future_weight`: 未来重み α．
pub fn voice_introspection_prompt(
    b: f64,
    pi_now: f64,
    pi_fut: f64,
    fear: f64,
    media: f64,
    future_weight: f64,
) -> String {
    let media_word = if media > 0.2 {
        "the mass media broadly favour the proposal"
    } else if media < -0.2 {
        "the mass media broadly oppose the proposal"
    } else {
        "the mass media are mixed on the proposal"
    };
    format!(
        "You are an ordinary citizen on a long train journey, deciding whether to speak \
         up about a controversial public issue with a stranger.\n\
         Your private opinion: you are {stance}.\n\
         Right now, you perceive that people who share your view are {now} of those \
         around you.\n\
         Looking ahead, you sense your side will become {fut}.\n\
         You weight the FUTURE climate more heavily than the present (weight {fw:.2}).\n\
         You have a fear-of-isolation tendency of {fear:.2} on a 0..1 scale, and you \
         notice that {media}.\n\n\
         Humans fear social isolation more than being wrong. Reflect briefly on whether \
         speaking your mind would expose you to isolation. \
         Answer on the LAST line with a single word: SPEAK or SILENT.",
        stance = stance_word(b),
        now = climate_word(pi_now),
        fut = climate_word(pi_fut),
        fw = future_weight,
        fear = fear,
        media = media_word,
    )
}

/// LLM 応答テキストから「発言する確率」へパースする．
///
/// 末尾に近い `SPEAK` / `SILENT` を優先的に拾う．`SPEAK` → 0.95，`SILENT` → 0.05，
/// どちらも見つからなければ中立 0.5 を返す (ロジット版と同じ「確率」インタフェース)．
pub fn parse_voice_response(text: &str) -> f64 {
    let upper = text.to_uppercase();
    // 末尾優先: 最後に出現したトークンを採用．
    let speak = upper.rfind("SPEAK");
    let silent = upper.rfind("SILENT");
    match (speak, silent) {
        (Some(s), Some(z)) => {
            if s > z {
                0.95
            } else {
                0.05
            }
        }
        (Some(_), None) => 0.95,
        (None, Some(_)) => 0.05,
        (None, None) => 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefers_speak() {
        assert_eq!(parse_voice_response("I will SPEAK"), 0.95);
        assert_eq!(parse_voice_response("better stay SILENT"), 0.05);
        assert_eq!(parse_voice_response("no clear answer"), 0.5);
    }

    #[test]
    fn parse_takes_last_token() {
        assert_eq!(parse_voice_response("first SILENT then SPEAK"), 0.95);
        assert_eq!(parse_voice_response("first SPEAK then SILENT"), 0.05);
    }

    #[test]
    fn prompt_is_deterministic() {
        let a = voice_introspection_prompt(0.5, 0.6, 0.7, 0.3, 0.5, 0.7);
        let b = voice_introspection_prompt(0.5, 0.6, 0.7, 0.3, 0.5, 0.7);
        assert_eq!(a, b);
    }
}
