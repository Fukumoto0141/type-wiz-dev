// ============================================
// src/save_data.rs
// セーブデータの構造と読み書きロジック
// ============================================

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;

const SAVE_FILE_NAME: &str = "save_data.json";

/// プレイヤーの進行状況データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerData {
    pub level: u32,
    pub current_xp: u32,
    pub total_typed_chars: u32,
    pub total_misses: u32,
}

impl Default for PlayerData {
    /// プレイヤーデータの初期値
    fn default() -> Self {
        Self {
            level: 1,
            current_xp: 0,
            total_typed_chars: 0,
            total_misses: 0,
        }
    }
}

impl PlayerData {
    /// 次のレベルまでに必要な経験値を計算する
    // ▼▼▼ (Task 1) レベルカーブの変更 ▼▼▼
    // 計算式: (レベル ^ 1.5) * 100
    pub fn required_xp_for_next_level(&self) -> u32 {
        // f64 で計算し、最後に u32 に丸める
        ((self.level as f64).powf(1.1) * 100.0).round() as u32
    }
    // ▲▲▲ (Task 1) 変更ここまで ▲▲▲

    /// 経験値を加算し、レベルアップ判定を行う
    // ▼▼▼ (Task 3) 獲得XPの仕様変更 ▼▼▼
    // `amount` (獲得XP) と `chars_typed` (タイプ文字数) を別々に受け取る
    pub fn add_xp(&mut self, xp_to_add: u32, chars_typed: u32) -> bool {
        self.current_xp += xp_to_add;
        self.total_typed_chars += chars_typed; // 累計タイプ数も加算
        // ▲▲▲ (Task 3) 変更ここまで ▲▲▲

        let mut leveled_up = false;
        // 必要経験値を超えている間、レベルを上げ続ける
        while self.current_xp >= self.required_xp_for_next_level() {
            self.current_xp -= self.required_xp_for_next_level();
            self.level += 1;
            leveled_up = true;
        }
        leveled_up
    }

    /// データをファイルに保存する (JSON)
    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(SAVE_FILE_NAME, json);
        }
    }

    /// ファイルからデータを読み込む
    pub fn load() -> Self {
        if !Path::new(SAVE_FILE_NAME).exists() {
            return Self::default();
        }

        if let Ok(file) = File::open(SAVE_FILE_NAME) {
            let reader = BufReader::new(file);
            if let Ok(data) = serde_json::from_reader(reader) {
                return data;
            }
        }
        Self::default()
    }
}