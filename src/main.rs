// ============================================
// src/main.rs (メインファイル)
// ============================================

use std::collections::HashMap;
use std::io::{Result, stdout};
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::{Parser, Subcommand};
use console::Term;
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
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Gauge},
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
        };
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

    if let Err(e) = update() {
        if e.to_string().contains("Access is denied") {
            eprintln!("エラー: 書き込み権限がありません。");
            eprintln!("管理者として実行するか、一度アンインストールして最新版をインストールし直してください。");
        } else {
            eprintln!("アップデート失敗: {}", e);
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

// --------------------------------------------------
// MARK:メニュー表示（通常スクリーン）
// --------------------------------------------------

fn show_menu(app_state: &mut AppState) -> Result<bool> {
    
    let term = Term::stdout();

    // タイトルロゴ
    println!();

    println!("\x1b[38;5;202m    ████████\x1b[38;5;166m╗\x1b[38;5;202m██\x1b[38;5;166m╗   \x1b[38;5;202m██\x1b[38;5;166m╗\x1b[38;5;202m██████\x1b[38;5;166m╗ \x1b[38;5;202m███████\x1b[38;5;166m╗\x1b[0m");

    println!("    \x1b[38;5;166m╚══\x1b[38;5;202m██\x1b[38;5;166m╔══╝╚\x1b[38;5;202m██\x1b[38;5;166m╗ \x1b[38;5;202m██\x1b[38;5;166m╔╝\x1b[38;5;202m██\x1b[38;5;166m╔══\x1b[38;5;202m██\x1b[38;5;166m╗\x1b[38;5;202m██\x1b[38;5;166m╔════╝\x1b[0m");

    println!("\x1b[38;5;202m       ██\x1b[38;5;166m║    ╚\x1b[38;5;202m████\x1b[38;5;166m╔╝ \x1b[38;5;202m██████\x1b[38;5;166m╔╝\x1b[38;5;202m█████\x1b[38;5;166m╗  \x1b[0m");

    println!("\x1b[38;5;202m       ██\x1b[38;5;166m║     ╚\x1b[38;5;202m██\x1b[38;5;166m╔╝  \x1b[38;5;202m██\x1b[38;5;166m╔═══╝ \x1b[38;5;202m██\x1b[38;5;166m╔══╝  \x1b[0m");

    println!("\x1b[38;5;202m       ██\x1b[38;5;166m║      \x1b[38;5;202m██\x1b[38;5;166m║   \x1b[38;5;202m██\x1b[38;5;166m║     \x1b[38;5;202m███████\x1b[38;5;166m╗\x1b[0m");

    println!("\x1b[38;5;166m       ╚═╝      ╚═╝   ╚═╝     ╚══════╝ \x1b[38;5;202mWiZ.\x1b[0m");

    println!();

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
        "Mission (Coming Soon...)",
        "Game Log",
        "Leaderboard (Coming Soon...)",
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
            
            app_state.mode = AppMode::Menu;
            term.clear_screen()?;

            Ok(false)
        }
        Some(2) => {
            // Game Log
            app_state.mode = AppMode::Log;
            Ok(true)
        }
        Some(5) | None => {
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
// MARK:ログ表示（通常スクリーン）
// --------------------------------------------------

fn show_log(app_state: &mut AppState) -> Result<()> {
    println!();
    println!("\x1b[36m═══════════════════════════════════════════════════════════════════════════\x1b[0m");
    println!("\x1b[36m  Game Log\x1b[0m");
    println!("\x1b[36m═══════════════════════════════════════════════════════════════════════════\x1b[0m");
    println!();
    
    if app_state.player_data.history.is_empty() {
        println!("\x1b[90m  No records yet. Start typing to create history!\x1b[0m");
    } else {
        let recent: Vec<_> = app_state
            .player_data
            .history
            .iter()
            .rev()
            .take(15)
            .collect();
        
        for record in recent {
            println!(
                "  {} | {} | CPS: {:.2} | Miss: {} | Score: {:.0}",
                record.timestamp.format("%m/%d %H:%M"),
                record.question_japanese,
                record.cps,
                record.misses,
                record.score
            );
        }
    }
    
    println!();
    println!("\x1b[90m  Press any key to return to menu...\x1b[0m");
    
    enable_raw_mode()?;
    loop {
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    disable_raw_mode()?;
                    app_state.mode = AppMode::Menu;
                    return Ok(());
                }
            }
        }
    }
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
}