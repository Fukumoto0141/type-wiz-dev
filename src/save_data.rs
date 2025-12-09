// ============================================
// src/save_data.rs
// セーブデータの構造と読み書きロジック
// ============================================

use bincode::config::standard;
use bincode::{Decode, Encode};
use chrono::{DateTime, TimeZone, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const SAVE_FILE_JSON: &str = "save_data.json"; // デバッグ用

/// 1回ごとのお題の記録
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRecord {
    pub timestamp: DateTime<Utc>,
    pub question_japanese: String,
    pub question_hiragana: String,
    pub total_chars: u32,
    pub duration_sec: f64,
    pub misses: u32,
    pub cps: f64,
    pub score: f64,
    pub xp_gained: u32,
}

/// bincode用の内部表現（DateTimeをi64に変換）
#[derive(Encode, Decode)]
struct TypeRecordBin {
    timestamp_secs: i64,
    question_japanese: String,
    question_hiragana: String,
    total_chars: u32,
    duration_sec: f64,
    misses: u32,
    cps: f64,
    score: f64,
    xp_gained: u32,
}

impl From<&TypeRecord> for TypeRecordBin {
    fn from(record: &TypeRecord) -> Self {
        Self {
            timestamp_secs: record.timestamp.timestamp(),
            question_japanese: record.question_japanese.clone(),
            question_hiragana: record.question_hiragana.clone(),
            total_chars: record.total_chars,
            duration_sec: record.duration_sec,
            misses: record.misses,
            cps: record.cps,
            score: record.score,
            xp_gained: record.xp_gained,
        }
    }
}

impl From<TypeRecordBin> for TypeRecord {
    fn from(bin: TypeRecordBin) -> Self {
        Self {
            timestamp: Utc.timestamp_opt(bin.timestamp_secs, 0).unwrap(),
            question_japanese: bin.question_japanese,
            question_hiragana: bin.question_hiragana,
            total_chars: bin.total_chars,
            duration_sec: bin.duration_sec,
            misses: bin.misses,
            cps: bin.cps,
            score: bin.score,
            xp_gained: bin.xp_gained,
        }
    }
}

/// プレイヤーの進行状況データ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerData {
    pub level: u32,
    pub current_xp: u32,
    pub total_typed_chars: u32,
    pub total_misses: u32,
    /// 過去のタイピング記録
    pub history: Vec<TypeRecord>,
}

/// bincode用の内部表現
#[derive(Encode, Decode)]
struct PlayerDataBin {
    level: u32,
    current_xp: u32,
    total_typed_chars: u32,
    total_misses: u32,
    history: Vec<TypeRecordBin>,
}

impl From<&PlayerData> for PlayerDataBin {
    fn from(data: &PlayerData) -> Self {
        Self {
            level: data.level,
            current_xp: data.current_xp,
            total_typed_chars: data.total_typed_chars,
            total_misses: data.total_misses,
            history: data.history.iter().map(TypeRecordBin::from).collect(),
        }
    }
}

impl From<PlayerDataBin> for PlayerData {
    fn from(bin: PlayerDataBin) -> Self {
        Self {
            level: bin.level,
            current_xp: bin.current_xp,
            total_typed_chars: bin.total_typed_chars,
            total_misses: bin.total_misses,
            history: bin.history.into_iter().map(TypeRecord::from).collect(),
        }
    }
}

impl Default for PlayerData {
    /// プレイヤーデータの初期値
    fn default() -> Self {
        Self {
            level: 1,
            current_xp: 0,
            total_typed_chars: 0,
            total_misses: 0,
            history: Vec::new(),
        }
    }
}

impl PlayerData {
    // MARK:セーブファイルのパスを取得する関数
    fn get_save_file_path() -> PathBuf {
        // "jp" (国), "MySchool" (組織名), "TypingGame" (アプリ名)
        // 組織名は適当でOKですが、ユニークな名前空間を作るために使われます
        if let Some(proj_dirs) = ProjectDirs::from("jp", "Fukumoto0141", "TYPE_WIZ") {
            // OSごとのデータ保存用ディレクトリパスを取得
            let data_dir = proj_dirs.data_dir();

            // ディレクトリがまだなければ作成する（これ重要！）
            if !data_dir.exists() {
                fs::create_dir_all(data_dir).expect("データディレクトリの作成に失敗しました");
            }

            // パスとファイル名を結合して返す
            return data_dir.join("save_data.bin");
        }

        // 万が一取得できなかったらカレントディレクトリに（フォールバック）
        PathBuf::from("save_data.bin")
    }

    /// 次のレベルまでに必要な経験値を計算する
    pub fn required_xp_for_next_level(&self) -> u32 {
        ((self.level as f64).powf(1.1) * 10.0).round() as u32
    }

    /// 経験値を加算し、レベルアップ判定を行う
    // `xp_to_add` (獲得XP) と `chars_typed` (タイプ文字数) を別々に受け取る
    pub fn add_xp(&mut self, xp_to_add: u32, chars_typed: u32) -> bool {
        self.current_xp += xp_to_add;
        self.total_typed_chars += chars_typed; // 累計タイプ数も加算

        let mut leveled_up = false;
        // 必要経験値を超えている間、レベルを上げ続ける
        while self.current_xp >= self.required_xp_for_next_level() {
            self.current_xp -= self.required_xp_for_next_level();
            self.level += 1;
            leveled_up = true;
        }
        leveled_up
    }

    /// MARK:データをファイルに保存する (バイナリ + JSON)
    pub fn save(&self) {
        let path = Self::get_save_file_path(); // ← パスを取得

        // --- 1. バイナリ形式で保存 (本番用) ---
        if let Ok(file) = File::create(&path) {
            let mut writer = BufWriter::new(file);
            let config = standard();
            let bin_data = PlayerDataBin::from(self);
            if let Ok(encoded) = bincode::encode_to_vec(&bin_data, config) {
                let _ = writer.write_all(&encoded);
            }
        }

        // --- 2. JSON形式で保存 (デバッグ用) ---
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(SAVE_FILE_JSON, json);
        }
    }

    /// MARK:ファイルからデータを読み込む (バイナリ優先、JSONフォールバック)
    pub fn load() -> Self {
        let path = Self::get_save_file_path(); // ← パスを取得

        // 1. バイナリファイルから読み込みを試行
        if Path::new(&path).exists() {
            if let Ok(mut file) = File::open(&path) {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    let config = standard();
                    if let Ok((bin_data, _)) =
                        bincode::decode_from_slice::<PlayerDataBin, _>(&buffer, config)
                    {
                        return PlayerData::from(bin_data);
                    }
                }
            }
        }

        // 2. バイナリ失敗時、JSONファイルから読み込みを試行 (古いセーブデータからの移行用)
        if Path::new(SAVE_FILE_JSON).exists() {
            if let Ok(file) = File::open(SAVE_FILE_JSON) {
                let reader = BufReader::new(file);
                if let Ok(data) = serde_json::from_reader(reader) {
                    return data;
                }
            }
        }

        // どちらも失敗した場合はデフォルト
        Self::default()
    }
}
