// ============================================
// src/main.rs (メインファイル)
// ============================================

use std::collections::HashMap;
use std::io::{Result, stdout};
use std::process::Command;
use std::time::{Duration, Instant};


use chrono::{Utc, Datelike};
use clap::{Parser, Subcommand};
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    cursor::Hide,
};
use dialoguer::{theme::ColorfulTheme, Select};
use rand::seq::SliceRandom;
use ratatui::{
    prelude::*,
    backend::CrosstermBackend,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Gauge, Sparkline, Table, Row, Cell, Clear},
    widgets::calendar::{CalendarEventStore, Monthly},
    symbols,
    widgets::{Axis, Chart, Dataset, GraphType},
};

// `src/questions.rs` をモジュールとして読み込む
mod questions;
use questions::{QUESTIONS_LIST, Question};

// `src/roman_mapping.rs` をモジュールとして読み込む
mod roman_mapping;
use roman_mapping::create_roman_mapping;

// `src/save_data.rs` をモジュールとして読み込む
mod save_data;
use save_data::{PlayerData, TypeRecord};

// `src/update.rs` をモジュールとして読み込む
mod update;
use update::update;

// --------------------------------------------------
// アプリケーションモード
// --------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum AppMode {
    Menu,
    Typing,
    Log,
    Exit,
}

// --------------------------------------------------
// ログ画面の状態
// --------------------------------------------------

#[derive(Debug, Clone)]
struct LogState {
    selected_year: i32,                // カレンダーで選択中の年
    selected_month: u32,               // カレンダーで選択中の月
    selected_day: Option<u32>,         // カレンダーで選択中の日
    show_detail_popup: bool,           // 詳細ポップアップ表示フラグ
    _chart_index: usize,               // 選択中のチャートインデックス
    date_input: String,                // 数字入力による日付選択
}

impl Default for LogState {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            selected_year: now.year(),
            selected_month: now.month(),
            selected_day: None,
            show_detail_popup: false,
            _chart_index: 0,
            date_input: String::new(),
        }
    }
}

// --------------------------------------------------
// MARK:コマンドライン引数
// --------------------------------------------------
#[derive(Parser)]
#[command(version, about, disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand,)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// タイピングゲームを開始
    #[command(visible_aliases = ["S","s"])]
    Start,
    /// ゲームログを表示
    #[command(visible_aliases = ["L","l"])]
    Log,
}

// --------------------------------------------------
// データ構造
// --------------------------------------------------

/// 「タイピング単位」（例：「し」「きゃ」）の状態を管理する
#[derive(Debug, Clone)]
struct CharState {
    _hiragana: String,          // "し" や "きゃ"
    patterns: Vec<String>,      // ["si", "shi", "ci"]
    current_pattern_idx: usize, // 今 "shi" を入力中など
    typed_count: usize,         // "shi" の "s" まで入力済みなら 1
}

impl CharState {
    fn new(hiragana: String, patterns: Vec<String>) -> Self {
        Self {
            _hiragana: hiragana,
            patterns,
            current_pattern_idx: 0,
            typed_count: 0,
        }
    }
    
    /// 現在アクティブなローマ字パターン（例: "shi"）を返す
    fn current_pattern(&self) -> &str {
        &self.patterns[self.current_pattern_idx]
    }
    
    /// この CharState が完了したか（例: "shi" を3文字打ち終わったか）
    fn is_complete(&self) -> bool {
        self.typed_count >= self.current_pattern().len()
    }
    
    /// 現在のパターンで、まだタイプしていない残りの部分（例: "hi"）
    fn remaining(&self) -> &str {
        &self.current_pattern()[self.typed_count..]
    }
}

/// MARK:アプリ全体の状態を管理する
struct AppState<'a> {
    mode: AppMode,
    _menu_index: usize,         // メニューの選択インデックス
    
    questions: Vec<&'a Question>,     // お題リストへの参照
    current_question_index: usize, // 今何問目か
    
    /// お題を CharState に分解したリスト
    char_states: Vec<CharState>,
    /// 現在タイプ中の CharState のインデックス
    current_char_index: usize,
    
    is_error: bool,              // ミスタイプ中か
    start_time: Option<Instant>, // タイマー開始時刻
    
    // 直前のリザルト表示用
    last_cps: Option<f64>, // (CPS表示用)
    last_time: Option<f64>,
    
    /// 現在のお題でのミス回数
    current_misses: u32,
    /// 直前のお題のミス回数
    last_misses: Option<u32>,
    /// 直前のお題のスコア
    last_score: Option<f64>,
    /// 直前に獲得した経験値
    last_xp_gained: Option<u32>,

    /// ローマ字辞書
    roman_map: HashMap<&'static str, Vec<&'static str>>,

    /// プレイヤーデータ
    player_data: PlayerData,

    /// Sparkline用のデータ（最近のCPS履歴）
    cps_history: Vec<u64>,
    /// Sparkline用のデータ（最近のスコア履歴）
    score_history: Vec<u64>,
    /// Sparklineの表示モード: true = CPS, false = Score
    show_cps_graph: bool,

    /// ログ画面の状態
    log_state: LogState,
}

impl<'a> AppState<'a> {
    /// AppState の初期化
    fn new() -> Self {
        let mut rng = rand::rng();
        let mut questions: Vec<&Question> = QUESTIONS_LIST.iter().collect();
        questions.shuffle(&mut rng);

        let mut state = Self {
            mode: AppMode::Menu,
            _menu_index: 0,
            
            questions,
            current_question_index: 0,
            char_states: Vec::new(),
            current_char_index: 0,
            is_error: false,
            start_time: None,
            last_cps: None,
            last_time: None,
            
            current_misses: 0,
            last_misses: None,
            last_score: None,
            last_xp_gained: None,

            roman_map: create_roman_mapping(),
            player_data: PlayerData::load(),

            cps_history: Vec::new(),
            score_history: Vec::new(),
            show_cps_graph: true,

            log_state: LogState::default(),
        };
        
        // 過去の記録から履歴データを読み込む（最新100件）
        let recent_records: Vec<_> = state.player_data.history.iter()
            .rev()
            .take(200)
            .rev()
            .collect();
        
        for record in recent_records {
            state.cps_history.push(record.cps.round() as u64);
            state.score_history.push(record.score.round() as u64);
        }
        
        state.load_current_question();
        state
    }
    
    /// 現在のお題を読み込み、`char_states` に分解する
    fn load_current_question(&mut self) {
        let question = self.questions[self.current_question_index];
        self.char_states = self.parse_hiragana(question.hiragana);
        self.current_char_index = 0;
        self.is_error = false;
        self.current_misses = 0;
    }
    
    /// ひらがな文字列を `Vec<CharState>` に分解（パース）する
    fn parse_hiragana(&self, text: &str) -> Vec<CharState> {
        let mut result = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut idx = 0;
        
        while idx < chars.len() {
            let mut found = false;

            // 3文字チェック
            if idx + 2 < chars.len() {
                let tri: String = chars[idx..=idx + 2].iter().collect();
                if let Some(patterns) = self.roman_map.get(tri.as_str()) {
                    result.push(CharState::new(
                        tri,
                        patterns.iter().map(|s| s.to_string()).collect(),
                    ));
                    idx += 3;
                    found = true;
                }
            }

            // 2文字チェック
            if !found && idx + 1 < chars.len() {
                let bi: String = chars[idx..=idx + 1].iter().collect();
                if let Some(patterns) = self.roman_map.get(bi.as_str()) {
                    result.push(CharState::new(
                        bi,
                        patterns.iter().map(|s| s.to_string()).collect(),
                    ));
                    idx += 2;
                    found = true;
                }
            }

            // 1文字チェック
            if !found {
                let uni = chars[idx].to_string();
                if let Some(patterns) = self.roman_map.get(uni.as_str()) {
                    result.push(CharState::new(
                        uni,
                        patterns.iter().map(|s| s.to_string()).collect(),
                    ));
                    idx += 1;
                } else {
                    idx += 1;
                }
            }
        }
        result
    }

    /// 表示用の日本語（漢字混じり）を返す
    fn get_current_question(&self) -> &'a Question {
        self.questions[self.current_question_index]
    }
    
    /// キー入力の処理
    fn handle_char_input(&mut self, c: char) {
        // タイマー開始
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
         // すべて打ち終わっている
        if self.current_char_index >= self.char_states.len() {
            return;
        }
        
        let current_state = &mut self.char_states[self.current_char_index];
        let expected_char = current_state.remaining().chars().next();
        
        if Some(c) == expected_char {
            current_state.typed_count += 1;
            self.is_error = false;
            // 次の CharState へ
            if current_state.is_complete() {
                self.current_char_index += 1;
            }
        } else {
            let mut found = false;
            let typed_so_far = &current_state.current_pattern()[..current_state.typed_count];
            
            for (i, pattern) in current_state.patterns.iter().enumerate() {
                if i == current_state.current_pattern_idx {
                    continue;
                }
                
                if pattern.starts_with(typed_so_far) {
                    if Some(c) == pattern.chars().nth(current_state.typed_count) {
                        current_state.current_pattern_idx = i;
                        current_state.typed_count += 1;
                        self.is_error = false;
                        found = true;
                        
                        if current_state.is_complete() {
                            self.current_char_index += 1;
                        }
                        break;
                    }
                }
            }

            if !found {
                self.is_error = true;
                self.current_misses += 1;
            }
        }
    }
    
    /// Backspace の処理
    fn handle_backspace(&mut self) {
        if self.current_char_index >= self.char_states.len() && self.current_char_index > 0 {
            self.current_char_index -= 1;
        }
        
        if self.current_char_index < self.char_states.len() {
            let current = &mut self.char_states[self.current_char_index];
            if current.typed_count > 0 {
                current.typed_count -= 1;
            } else if self.current_char_index > 0 {
                self.current_char_index -= 1;
                let prev_len = self.char_states[self.current_char_index]
                    .current_pattern()
                    .len();
                self.char_states[self.current_char_index].typed_count = prev_len.saturating_sub(1);
            }
        }
        self.is_error = false;
    }
    
    /// お題をすべて打ち終わったか
    fn is_question_complete(&self) -> bool {
        self.current_char_index >= self.char_states.len()
    }
    
    /// 次のお題に進む
    fn next_question(&mut self) {
        if let Some(start) = self.start_time {
            let duration = start.elapsed();
            let duration_sec = duration.as_secs_f64();
            let total_chars: usize = self
                .char_states
                .iter()
                .map(|cs| cs.current_pattern().len())
                .sum();
            
            let misses = self.current_misses;
            let total_attempts = (total_chars as u32 + misses) as f64;
            let accuracy = if total_attempts > 0.0 {
                (total_chars as f64 / total_attempts) * 100.0
            } else {
                100.0
            };

            let mut cps = 0.0;
            if duration_sec > 0.0 {
                cps = total_chars as f64 / duration_sec;
            }

            let score = (cps * 100.0) * (accuracy / 100.0).powi(3) * (total_chars as f64);

            let base_xp = total_chars as f64;
            let skill_bonus = 1.0 + (cps / 10.0);
            let accuracy_mod = (accuracy / 100.0).powi(3);
            let final_xp = (base_xp * skill_bonus * accuracy_mod).round() as u32;

            self.last_cps = Some(cps);
            self.last_time = Some(duration_sec);
            self.last_misses = Some(misses);
            self.last_score = Some(score);
            self.last_xp_gained = Some(final_xp);

            let question = self.get_current_question();
            let record = TypeRecord {
                timestamp: Utc::now(),
                question_japanese: question.japanese.to_string(),
                question_hiragana: question.hiragana.to_string(),
                total_chars: total_chars as u32,
                duration_sec,
                misses,
                cps,
                score,
                xp_gained: final_xp,
            };
            self.player_data.history.push(record);

            self.player_data.add_xp(final_xp, total_chars as u32);
            self.player_data.total_misses += misses;
            self.player_data.save();

            // Sparkline用の履歴データを更新（最新100件まで保持）
            self.cps_history.push(cps.round() as u64);
            if self.cps_history.len() > 200 {
                self.cps_history.remove(0);
            }
            self.score_history.push(score.round() as u64);
            if self.score_history.len() > 200 {
                self.score_history.remove(0);
            }
        }
        
        self.current_question_index = (self.current_question_index + 1) % self.questions.len();
        self.load_current_question();
        self.start_time = None;
    }
}

// --------------------------------------------------
// MARK:メイン関数
// --------------------------------------------------

fn main() -> Result<()> {
    let mut app_state = AppState::new();

    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Start) =>  app_state.mode = AppMode::Typing,
        Some(Commands::Log) => app_state.mode = AppMode::Log,
        // デフォルトの挙動
        None => app_state.mode = AppMode::Menu,
    }

    match update() {
        Ok(status) => {
            if status.updated() {
                if let Err(e) = relaunch_current_exe() {
                    eprintln!("再起動に失敗しました: {}", e);
                }
                std::process::exit(0);
            }
        }
        Err(e) => {
            if e.to_string().contains("Access is denied") {
                eprintln!("エラー: 書き込み権限がありません。");
                eprintln!("管理者として実行するか、一度アンインストールして最新版をインストールし直してください。");
            } else {
                eprintln!("アップデート失敗: {}", e);
            }
        }
    }

    loop {
        match app_state.mode {
            AppMode::Menu => {
                if !show_menu(&mut app_state)? {
                    // falseだった時の処理
                }
            }
            AppMode::Typing => {
                run_typing_mode(&mut app_state)?;
            }
            AppMode::Log => {
                show_log(&mut app_state)?;
            }
            AppMode::Exit => {
                break;
            }
        }
    }
    
    Ok(())
}

fn relaunch_current_exe() -> Result<()> {
    let exe = std::env::current_exe()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    Command::new(exe).args(args).status()?;
    Ok(())
}

// --------------------------------------------------
// MARK:メニュー表示（通常スクリーン）
// --------------------------------------------------

fn show_menu(app_state: &mut AppState) -> Result<bool> {
    // タイトルロゴ
    println!();

    println!("\x1b[38;5;202m    ████████\x1b[38;5;166m╗\x1b[38;5;202m██\x1b[38;5;166m╗   \x1b[38;5;202m██\x1b[38;5;166m╗\x1b[38;5;202m██████\x1b[38;5;166m╗ \x1b[38;5;202m███████\x1b[38;5;166m╗\x1b[0m");

    println!("    \x1b[38;5;166m╚══\x1b[38;5;202m██\x1b[38;5;166m╔══╝╚\x1b[38;5;202m██\x1b[38;5;166m╗ \x1b[38;5;202m██\x1b[38;5;166m╔╝\x1b[38;5;202m██\x1b[38;5;166m╔══\x1b[38;5;202m██\x1b[38;5;166m╗\x1b[38;5;202m██\x1b[38;5;166m╔════╝\x1b[0m");

    println!("\x1b[38;5;202m       ██\x1b[38;5;166m║    ╚\x1b[38;5;202m████\x1b[38;5;166m╔╝ \x1b[38;5;202m██████\x1b[38;5;166m╔╝\x1b[38;5;202m█████\x1b[38;5;166m╗  \x1b[0m");

    println!("\x1b[38;5;202m       ██\x1b[38;5;166m║     ╚\x1b[38;5;202m██\x1b[38;5;166m╔╝  \x1b[38;5;202m██\x1b[38;5;166m╔═══╝ \x1b[38;5;202m██\x1b[38;5;166m╔══╝  \x1b[0m");

    println!("\x1b[38;5;202m       ██\x1b[38;5;166m║      \x1b[38;5;202m██\x1b[38;5;166m║   \x1b[38;5;202m██\x1b[38;5;166m║     \x1b[38;5;202m███████\x1b[38;5;166m╗\x1b[0m");

    println!("\x1b[38;5;166m       ╚═╝      ╚═╝   ╚═╝     ╚══════╝ \x1b[38;5;202mWiZ.\x1b[0m");

    println!();

    let items = vec![
        "Start Type",
        "Game Log",
        "Settings (Coming Soon...)",
        "Exit",
    ];
    
    let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&items)
        .default(app_state._menu_index | 0)
        .interact_opt()?;

    match selection {
        Some(0) => {
            app_state.mode = AppMode::Typing;
            Ok(true)
        }
        Some(1) => {
            // Game Log
            app_state.mode = AppMode::Log;
            Ok(true)
        }
        Some(3) | None => {
            // Exit or Esc
            app_state.mode = AppMode::Exit;
            Ok(false)
        }
        _ => {
            // Coming Soon...
            // show_menu(app_state)?;
            app_state.mode = AppMode::Menu;
            Ok(false)
        }
    }
}

// --------------------------------------------------
// MARK:タイピングモード（代替スクリーン）
// --------------------------------------------------

fn run_typing_mode(app_state: &mut AppState) -> Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?; // 代替スクリーンを使用
    stdout().execute(Hide)?; // カーソルを非表示
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| ui_typing(f, app_state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => {
                            // stdout().execute(Show)?;
                            stdout().execute(LeaveAlternateScreen)?;
                            disable_raw_mode()?;
                            app_state.mode = AppMode::Exit;
                            app_state.load_current_question();
                            return Ok(());
                        }
                        KeyCode::Backspace => app_state.handle_backspace(),
                        KeyCode::Tab => {
                            // Sparklineの表示切り替え
                            app_state.show_cps_graph = !app_state.show_cps_graph;
                        }
                        KeyCode::Char(c) => {
                            app_state.handle_char_input(c);
                            if app_state.is_question_complete() {
                                app_state.next_question();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

// --------------------------------------------------
// MARK:ログ表示（TUI版）
// --------------------------------------------------

fn show_log(app_state: &mut AppState) -> Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(Hide)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    loop {
        terminal.draw(|f| ui_log(f, app_state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    if handle_log_key(key.code, app_state) {
                        break;
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    app_state.mode = AppMode::Menu;
    Ok(())
}

// --------------------------------------------------
// UI描画 - タイピング
// --------------------------------------------------

fn ui_typing(f: &mut Frame, app_state: &AppState) {
    let size = f.area();
    let block = Block::default().borders(Borders::ALL).title(" TYPE WiZ ");
    let inner_area = block.inner(size);
    f.render_widget(block, size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(4),
        ])
        .split(inner_area);

    // ステータスバー
    let pd = &app_state.player_data;
    let req_xp = pd.required_xp_for_next_level();
    let ratio = if req_xp > 0 {
        (pd.current_xp as f64 / req_xp as f64).min(1.0)
    } else {
        0.0
    };

    let xp_text = match app_state.last_xp_gained {
        Some(xp) => format!(" +{}XP", xp),
        None => String::new(),
    };
    
    let label = format!("Lv.{} ({} / {}) {}", pd.level, pd.current_xp, req_xp, xp_text);
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(Color::Magenta).bg(Color::Black))
        .ratio(ratio)
        .label(label);
    f.render_widget(gauge, chunks[0]);

    // リザルト
    let cps_time_text = match (app_state.last_cps, app_state.last_time) {
        (Some(cps), Some(time)) => format!("CPS: {:.2} / Time: {:.2}s", cps, time),
        _ => String::new(),
    };
    let score_miss_text = match (app_state.last_score, app_state.last_misses) {
        (Some(score), Some(misses)) => format!("Score: {:.0} / Miss: {}", score, misses),
        _ => String::new(),
    };

    let result_paragraph = Paragraph::new(vec![
        Line::from(cps_time_text).style(Style::default().fg(Color::Yellow)),
        Line::from(score_miss_text).style(Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(result_paragraph, chunks[1]);

    // 日本語
    f.render_widget(
        Paragraph::new(app_state.get_current_question().japanese)
            .style(Style::default().fg(Color::White).bold())
            .centered(),
        chunks[2],
    );
    
    // ひらがな
    f.render_widget(
        Paragraph::new(app_state.get_current_question().hiragana)
            .style(Style::default().fg(Color::Gray))
            .centered(),
        chunks[4],
    );

    // ローマ字
    let mut spans = Vec::new();
    for (i, cs) in app_state.char_states.iter().enumerate() {
        let pattern = cs.current_pattern(); 
        
        if i < app_state.current_char_index {
            spans.push(Span::styled(pattern, Style::default().fg(Color::Green)));
        } else if i == app_state.current_char_index {
            let typed = &pattern[..cs.typed_count];
            let remaining = &pattern[cs.typed_count..];
            
            if !typed.is_empty() {
                spans.push(Span::styled(typed, Style::default().fg(Color::Green)));
            }
            
            if let Some(next) = remaining.chars().next() {
                let style = if app_state.is_error {
                    Style::default().fg(Color::White).bg(Color::Red)
                } else {
                    Style::default().fg(Color::Black).bg(Color::White)
                };
                spans.push(Span::styled(next.to_string(), style));
                
                if remaining.len() > 1 {
                    spans.push(Span::styled(
                        &remaining[1..],
                        Style::default().fg(Color::Gray),
                    ));
                }
            }
        } else {
            spans.push(Span::styled(pattern, Style::default().fg(Color::DarkGray)));
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).centered(),
        chunks[5]
    );

    // Sparkline（タイピング速度またはスコアの推移グラフ）
    let (graph_data, graph_title, graph_color) = if app_state.show_cps_graph {
        (&app_state.cps_history, " CPS History (Tab to switch) ", Color::Cyan)
    } else {
        (&app_state.score_history, " Score History (Tab to switch) ", Color::Yellow)
    };

    if !graph_data.is_empty() {
        // ウィンドウ幅に応じて表示するデータ数を動的に調整
        // Sparklineの幅からボーダー分(2)を引いた値を最大データ数とする
        let available_width = chunks[6].width.saturating_sub(2) as usize;
        let data_to_show = if graph_data.len() <= available_width {
            graph_data.as_slice()
        } else {
            // 最新のデータから幅分だけ取得
            &graph_data[graph_data.len() - available_width..]
        };
        
        let sparkline = Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title(graph_title))
            .data(data_to_show)
            .style(Style::default().fg(graph_color));
        f.render_widget(sparkline, chunks[6]);
    } else {
        let placeholder = Paragraph::new("No data yet...")
            .block(Block::default().borders(Borders::ALL).title(graph_title))
            .style(Style::default().fg(Color::DarkGray))
            .centered();
        f.render_widget(placeholder, chunks[6]);
    }
}

// --------------------------------------------------
// UI描画 - ログ
// --------------------------------------------------

fn ui_log(f: &mut Frame, app_state: &AppState) {
    let size = f.area();
    let block = Block::default().borders(Borders::ALL).title(" TYPE WiZ - Log ");
    let inner_area = block.inner(size);
    f.render_widget(block, size);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(inner_area);

    ui_log_calendar_year(f, main_chunks[0], app_state);
    ui_log_growth_chart(f, main_chunks[1], app_state);

    // ヘルプテキスト
    let input_hint = if app_state.log_state.date_input.is_empty() {
        "MMDD または DD"
    } else {
        &app_state.log_state.date_input
    };
    let help_text = format!(
        " ←→: 月  ↑↓: 年  数字入力: 日付選択 ({})  Enter: 詳細  Backspace: 取消  q/Esc: 戻る ",
        input_hint
    );
    
    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .centered();
    
    let help_area = Rect {
        x: size.x + 1,
        y: size.y + size.height.saturating_sub(1),
        width: size.width.saturating_sub(2),
        height: 1,
    };
    f.render_widget(help, help_area);

    // 詳細ポップアップ
    if app_state.log_state.show_detail_popup {
        ui_log_detail_popup(f, app_state);
    }
}

fn ui_log_calendar_year(f: &mut Frame, area: Rect, app_state: &AppState) {
    let year = app_state.log_state.selected_year;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let mut month = 1u32;
    for row in rows.iter() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(*row);

        for col in cols.iter() {
            if month > 12 {
                break;
            }
            let is_current = app_state.log_state.selected_year == year
                && app_state.log_state.selected_month == month;
            render_calendar_widget(f, *col, app_state, year, month, is_current);
            month += 1;
        }
        if month > 12 {
            break;
        }
    }
}

fn render_calendar_widget(f: &mut Frame, area: Rect, app_state: &AppState, year: i32, month: u32, is_current: bool) {
    use chrono::Datelike;
    use time::{Date, Month};

    // 該当月のプレイ日を収集
    let mut event_dates = Vec::new();
    for record in &app_state.player_data.history {
        let record_date = record.timestamp.date_naive();
        if record_date.year() == year && record_date.month() == month {
            // NaiveDateからtime::Dateに変換
            if let Ok(time_month) = Month::try_from(month as u8) {
                if let Ok(time_date) = Date::from_calendar_date(year, time_month, record_date.day() as u8) {
                    event_dates.push(time_date);
                }
            }
        }
    }
    event_dates.sort();
    event_dates.dedup();

    // CalendarEventStoreを作成
    let mut event_store = CalendarEventStore::default();
    for date in event_dates {
        event_store.add(date, Style::default().fg(Color::Green).bold());
    }

    // 選択された日付
    if is_current &&
       app_state.log_state.selected_year == year &&
       app_state.log_state.selected_month == month {
        if let Some(day) = app_state.log_state.selected_day {
            if let Ok(time_month) = Month::try_from(month as u8) {
                if let Ok(selected_date) = Date::from_calendar_date(year, time_month, day as u8) {
                    event_store.add(selected_date, Style::default().fg(Color::Black).bg(Color::Yellow));
                }
            }
        }
    }

    // カレンダーの基準日
    let display_date = if let Ok(time_month) = Month::try_from(month as u8) {
        Date::from_calendar_date(year, time_month, 1).unwrap()
    } else {
        Date::from_calendar_date(year, Month::January, 1).unwrap()
    };

    let title = format!(" {}/{} ", year, month);
    let title_style = if is_current {
        Style::default().fg(Color::Cyan).bold()
    } else {
        Style::default().fg(Color::Gray)
    };

    // Calendarウィジェットを作成
    let calendar = Monthly::new(display_date, event_store)
        .block(Block::default().borders(Borders::ALL).title(title).title_style(title_style))
        .show_month_header(Style::default().fg(Color::Cyan))
        .show_weekdays_header(Style::default().bold())
        .default_style(Style::default().fg(Color::Gray));

    f.render_widget(calendar, area);
}

fn ui_log_growth_chart(f: &mut Frame, area: Rect, app_state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(area);

    let history_len = app_state.player_data.history.len();
    if history_len == 0 {
        let no_data = Paragraph::new("No data available")
            .style(Style::default().fg(Color::DarkGray))
            .centered()
            .block(Block::default().borders(Borders::ALL).title(" Growth Charts "));
        f.render_widget(no_data, area);
        return;
    }

    // 成長の推移: 累積平均CPS / 累積平均スコア
    let mut cps_sum = 0.0f64;
    let mut score_sum = 0.0f64;
    let mut growth_cps: Vec<(f64, f64)> = Vec::new();
    let mut growth_score: Vec<(f64, f64)> = Vec::new();

    for (i, r) in app_state.player_data.history.iter().enumerate() {
        cps_sum += r.cps;
        score_sum += r.score;
        let idx = i as f64;
        growth_cps.push((idx, cps_sum / (i as f64 + 1.0)));
        growth_score.push((idx, score_sum / (i as f64 + 1.0)));
    }

    let cps_max = growth_cps.iter().map(|(_, y)| *y).fold(0.0f64, f64::max);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(chunks[0]);

    let cps_dataset = Dataset::default()
        .name("Avg CPS")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&growth_cps);

    let cps_chart = Chart::new(vec![cps_dataset])
        .block(Block::default().borders(Borders::ALL).title(" Avg CPS (Cumulative) "))
        .x_axis(
            Axis::default()
                .title("Session")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (history_len as f64).max(1.0)])
        )
        .y_axis(
            Axis::default()
                .title("CPS")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (cps_max * 1.1).max(1.0)])
        );
    f.render_widget(cps_chart, top_cols[0]);

    let score_scaled: Vec<(f64, f64)> = growth_score
        .iter()
        .map(|(x, y)| (*x, *y / 100.0))
        .collect();
    let score_scaled_max = score_scaled.iter().map(|(_, y)| *y).fold(0.0f64, f64::max);

    let score_dataset = Dataset::default()
        .name("Avg Score/100")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Yellow))
        .data(&score_scaled);

    let score_chart = Chart::new(vec![score_dataset])
        .block(Block::default().borders(Borders::ALL).title(" Avg Score (Cumulative, /100) "))
        .x_axis(
            Axis::default()
                .title("Session")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (history_len as f64).max(1.0)])
        )
        .y_axis(
            Axis::default()
                .title("Score")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (score_scaled_max * 1.1).max(1.0)])
        );
    f.render_widget(score_chart, top_cols[1]);

    // 日別XP（棒グラフ）
    let mut day_map: Vec<(chrono::NaiveDate, u32)> = Vec::new();
    for record in &app_state.player_data.history {
        let d = record.timestamp.date_naive();
        if let Some(pos) = day_map.iter().position(|(date, _)| *date == d) {
            day_map[pos].1 += record.xp_gained;
        } else {
            day_map.push((d, record.xp_gained));
        }
    }
    day_map.sort_by_key(|(d, _)| *d);

    let xp_data: Vec<(f64, f64)> = day_map
        .iter()
        .enumerate()
        .map(|(i, (_, xp))| (i as f64, *xp as f64))
        .collect();

    let xp_max = xp_data.iter().map(|(_, y)| *y).fold(0.0f64, f64::max);
    let xp_dataset = Dataset::default()
        .name("Daily XP")
        .graph_type(GraphType::Bar)
        .style(Style::default().fg(Color::Magenta))
        .data(&xp_data);

    let xp_chart = Chart::new(vec![xp_dataset])
        .block(Block::default().borders(Borders::ALL).title(" Daily XP (Bar) "))
        .x_axis(
            Axis::default()
                .title("Day Index")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (xp_data.len() as f64).max(1.0)])
        )
        .y_axis(
            Axis::default()
                .title("XP")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, (xp_max * 1.1).max(1.0)])
        );
    f.render_widget(xp_chart, chunks[1]);
}

fn ui_log_detail_popup(f: &mut Frame, app_state: &AppState) {
    use chrono::Datelike;
    
    if let Some(day) = app_state.log_state.selected_day {
        let year = app_state.log_state.selected_year;
        let month = app_state.log_state.selected_month;

        // 該当日のデータを取得
        let records: Vec<&TypeRecord> = app_state.player_data.history.iter()
            .filter(|r| {
                let date = r.timestamp.date_naive();
                date.year() == year && date.month() == month && date.day() == day
            })
            .collect();

        if records.is_empty() {
            return;
        }

        // ポップアップエリア
        let area = f.area();
        let popup_width = area.width.min(80);
        let popup_height = area.height.min(20);
        let popup_area = Rect {
            x: (area.width.saturating_sub(popup_width)) / 2,
            y: (area.height.saturating_sub(popup_height)) / 2,
            width: popup_width,
            height: popup_height,
        };

        // 背景をクリア
        f.render_widget(Clear, popup_area);
        f.render_widget(
            Block::default()
                .style(Style::default().bg(Color::Black)),
            popup_area
        );

        // テーブル作成
        let mut rows = vec![
            Row::new(vec!["Time", "Question", "CPS", "Miss", "Score"])
                .style(Style::default().fg(Color::Cyan).bold())
        ];

        for record in records.iter().take(15) {
            rows.push(Row::new(vec![
                Cell::from(record.timestamp.format("%H:%M:%S").to_string()),
                Cell::from(record.question_japanese.clone()),
                Cell::from(format!("{:.2}", record.cps)),
                Cell::from(format!("{}", record.misses)),
                Cell::from(format!("{:.0}", record.score)),
            ]));
        }

        let table = Table::new(
            rows,
            [
                Constraint::Length(10),
                Constraint::Min(20),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Length(10),
            ]
        )
        .block(Block::default()
            .borders(Borders::ALL)
            .title(format!(" Details: {}/{}/{} (Press ESC to close) ", year, month, day))
        );

        f.render_widget(table, popup_area);
    }
}

// --------------------------------------------------
// イベント処理 - ログ
// --------------------------------------------------

fn handle_log_key(key: KeyCode, app_state: &mut AppState) -> bool {
    // ポップアップ表示中の処理
    if app_state.log_state.show_detail_popup {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                app_state.log_state.show_detail_popup = false;
            }
            _ => {}
        }
        return false;
    }

    // 通常の処理
    match key {
        KeyCode::Char('q') | KeyCode::Esc => {
            return true; // ログ画面を終了
        }
        KeyCode::Left => {
            if app_state.log_state.selected_month > 1 {
                app_state.log_state.selected_month -= 1;
            }
        }
        KeyCode::Right => {
            if app_state.log_state.selected_month < 12 {
                app_state.log_state.selected_month += 1;
            }
        }
        KeyCode::Up => {
            app_state.log_state.selected_year -= 1;
        }
        KeyCode::Down => {
            app_state.log_state.selected_year += 1;
        }
        KeyCode::Backspace => {
            app_state.log_state.date_input.pop();
            apply_date_input(app_state);
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if app_state.log_state.date_input.len() < 4 {
                app_state.log_state.date_input.push(c);
                apply_date_input(app_state);
            }
        }
        KeyCode::Char('/') | KeyCode::Char('-') => {
            // 区切り文字は無視
        }
        KeyCode::Enter => {
            if app_state.log_state.selected_day.is_some() {
                app_state.log_state.show_detail_popup = true;
            }
        }
        _ => {}
    }
    
    false
}

fn apply_date_input(app_state: &mut AppState) {
    let s = app_state.log_state.date_input.clone();
    if s.is_empty() {
        return;
    }

    let mut month = app_state.log_state.selected_month;
    let mut day_opt: Option<u32> = None;

    if s.len() <= 2 {
        if let Ok(day) = s.parse::<u32>() {
            day_opt = Some(day);
        }
    } else if s.len() == 3 {
        let m = &s[0..1];
        let d = &s[1..3];
        if let (Ok(mv), Ok(dv)) = (m.parse::<u32>(), d.parse::<u32>()) {
            month = mv;
            day_opt = Some(dv);
        }
    } else if s.len() >= 4 {
        let m = &s[0..2];
        let d = &s[2..4];
        if let (Ok(mv), Ok(dv)) = (m.parse::<u32>(), d.parse::<u32>()) {
            month = mv;
            day_opt = Some(dv);
        }
    }

    if let Some(day) = day_opt {
        if (1..=12).contains(&month) && (1..=31).contains(&day) {
            app_state.log_state.selected_month = month;
            app_state.log_state.selected_day = Some(day);
        }
    }
}