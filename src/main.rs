// ============================================
// src/main.rs (メインファイル)
// ============================================

use std::collections::HashMap;
use std::io::{Result, stdout};
use std::time::{Duration, Instant};

// `src/questions.rs` をモジュールとして読み込む
mod questions;
use questions::{QUESTIONS_LIST, Question};

// `src/roman_mapping.rs` をモジュールとして読み込む
mod roman_mapping;
use roman_mapping::create_roman_mapping;

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    prelude::*,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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
    /// 新しい CharState を作成
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

/// アプリ全体の状態を管理する
struct AppState<'a> {
    questions: &'a [Question],     // お題リストへの参照
    current_question_index: usize, // 今何問目か

    /// お題を CharState に分解したリスト
    char_states: Vec<CharState>,
    /// 現在タイプ中の CharState のインデックス
    current_char_index: usize,

    is_error: bool,              // ミスタイプ中か
    start_time: Option<Instant>, // タイマー開始時刻

    // 直前のリザルト表示用
    last_wpm: Option<f64>,
    last_time: Option<f64>,

    /// ローマ字辞書
    roman_map: HashMap<&'static str, Vec<&'static str>>,
}

impl<'a> AppState<'a> {
    /// AppState の初期化
    fn new() -> Self {
        let mut state = Self {
            questions: QUESTIONS_LIST,
            current_question_index: 0,
            char_states: Vec::new(),
            current_char_index: 0,
            is_error: false,
            start_time: None,
            last_wpm: None,
            last_time: None,
            roman_map: create_roman_mapping(), // `roman_mapping` モジュールから辞書作成
        };
        state.load_current_question(); // 最初のお題を読み込む
        state
    }

    /// 現在のお題を読み込み、`char_states` に分解する
    fn load_current_question(&mut self) {
        let question = &self.questions[self.current_question_index];

        // `question.hiragana` (タイピング用) を `parse_hiragana` に渡す
        self.char_states = self.parse_hiragana(question.hiragana);

        self.current_char_index = 0;
        self.is_error = false;
    }

    /// ひらがな文字列を `Vec<CharState>` に分解（パース）する
    fn parse_hiragana(&self, text: &str) -> Vec<CharState> {
        let mut result = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut idx = 0;

        // 最長一致でパースする
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
                    // 辞書にない文字 (漢字など) はスキップ
                    idx += 1;
                }
            }
        }

        result
    }

    /// 表示用の日本語（漢字混じり）を返す
    fn get_current_question(&self) -> &'a Question {
        &self.questions[self.current_question_index]
    }

    /// キー入力の処理
    fn handle_char_input(&mut self, c: char) {
        // タイマー開始
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }

        if self.current_char_index >= self.char_states.len() {
            return; // すべて打ち終わっている
        }

        let current_state = &mut self.char_states[self.current_char_index];
        let expected_char = current_state.remaining().chars().next();

        // 1. 現在のパターンで試す
        if Some(c) == expected_char {
            current_state.typed_count += 1;
            self.is_error = false;

            if current_state.is_complete() {
                self.current_char_index += 1; // 次の CharState へ
            }
        } else {
            // 2. 別のパターンで試す
            let mut found = false;
            let typed_so_far = &current_state.current_pattern()[..current_state.typed_count];

            for (i, pattern) in current_state.patterns.iter().enumerate() {
                if i == current_state.current_pattern_idx {
                    continue; // 今のパターンはもう試した
                }

                // "shi" が "s" (typed_so_far) で始まるか？
                if pattern.starts_with(typed_so_far) {
                    // "shi" の `typed_count` 番目('h')は、入力('h')と一致するか？
                    if Some(c) == pattern.chars().nth(current_state.typed_count) {
                        current_state.current_pattern_idx = i; // パターンを "shi" に切り替え
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

            // どのパターンにも合わなかった
            if !found {
                self.is_error = true;
            }
        }
    }

    /// Backspace の処理
    fn handle_backspace(&mut self) {
        // 既に完了しているが、まだ `next_question` が呼ばれていない場合
        if self.current_char_index >= self.char_states.len() && self.current_char_index > 0 {
            self.current_char_index -= 1;
        }

        if self.current_char_index < self.char_states.len() {
            let current = &mut self.char_states[self.current_char_index];
            if current.typed_count > 0 {
                // 現在の CharState の入力を1文字戻す
                current.typed_count -= 1;
            } else if self.current_char_index > 0 {
                // 前の CharState に戻る
                self.current_char_index -= 1;
                // 前の CharState の入力を最後から1文字削る
                let prev_len = self.char_states[self.current_char_index]
                    .current_pattern()
                    .len();
                self.char_states[self.current_char_index].typed_count = prev_len.saturating_sub(1);
            }
        }
        self.is_error = false; // Backspaceでエラーはリセット
    }

    /// お題をすべて打ち終わったか
    fn is_question_complete(&self) -> bool {
        self.current_char_index >= self.char_states.len()
    }

    /// 次のお題に進む
    fn next_question(&mut self) {
        // リザルトを計算して `last_...` に保存
        if let Some(start) = self.start_time {
            let duration = start.elapsed();
            let duration_sec = duration.as_secs_f64();
            // 実際にタイプしたローマ字の総数を計算
            let total_chars: usize = self
                .char_states
                .iter()
                .map(|cs| cs.current_pattern().len())
                .sum();

            if duration_sec > 0.0 {
                let wpm = (total_chars as f64 / 5.0) / (duration_sec / 60.0);
                self.last_wpm = Some(wpm);
                self.last_time = Some(duration_sec);
            }
        }

        // 次のお題へ
        self.current_question_index = (self.current_question_index + 1) % self.questions.len();
        self.load_current_question(); // ここで `char_states` がリセットされる
        self.start_time = None;
    }
}

// --------------------------------------------------
// メイン関数 (TUIセットアップと実行ループ)
// --------------------------------------------------

fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    run_app(&mut terminal)?;
    restore_terminal(&mut terminal)?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<impl Backend>> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?; // 代替スクリーンを使用
    let backend = CrosstermBackend::new(stdout());
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<impl Backend>) -> Result<()> {
    stdout().execute(LeaveAlternateScreen)?; // 代替スクリーンを終了
    disable_raw_mode()?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<impl Backend>) -> Result<()> {
    let mut app_state = AppState::new();

    loop {
        terminal.draw(|f| ui(f, &app_state))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => break,
                        KeyCode::Backspace => app_state.handle_backspace(),
                        KeyCode::Char(c) => {
                            app_state.handle_char_input(c);
                            // 完了したら自動で次へ
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

    Ok(())
}

// --------------------------------------------------
// UI描画
// --------------------------------------------------

fn ui(f: &mut Frame, app_state: &AppState) {
    let size = f.area();
    // 枠線を描画
    let block = Block::default().borders(Borders::ALL).title("Type Wiz !");
    let inner_area = block.inner(size);
    f.render_widget(block, size);

    // レイアウト (リザルト/日本語/空白/タイピング)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // [0] リザルト
            Constraint::Length(1), // [1] 日本語
            Constraint::Length(1), // [2] 空白
            Constraint::Length(1), // [3] ひらがな
            Constraint::Min(1),    // [3] タイピングエリア
        ])
        .split(inner_area);

    // 1. リザルト表示
    let result_text = match (app_state.last_wpm, app_state.last_time) {
        (Some(wpm), Some(time)) => format!("Last: WPM: {:.2} / Time: {:.2}s", wpm, time),
        _ => String::new(),
    };
    f.render_widget(
        Paragraph::new(result_text).style(Style::default().fg(Color::Yellow)),
        chunks[0],
    );

    // 2. 日本語（漢字混じり）表示
    f.render_widget(
        Paragraph::new(app_state.get_current_question().japanese)
            .style(Style::default().fg(Color::White).bold()),
        chunks[1],
    );

    // 3. ひらがな表示
    f.render_widget(
        Paragraph::new(app_state.get_current_question().hiragana)
            .style(Style::default().fg(Color::Gray)),
        chunks[3],
    );

    // 4. ローマ字タイピングエリア表示
    let mut spans = Vec::new();
    let mut cursor_pos = 0;

    // 全ての CharState をループして描画
    for (i, cs) in app_state.char_states.iter().enumerate() {
        // "si" や "shi" など、現在アクティブなパターン
        let pattern = cs.current_pattern();

        if i < app_state.current_char_index {
            // 完了済みの CharState (緑)
            spans.push(Span::styled(pattern, Style::default().fg(Color::Green)));
            cursor_pos += pattern.len();
        } else if i == app_state.current_char_index {
            // 現在の CharState (入力中)
            let typed = &pattern[..cs.typed_count];
            let remaining = &pattern[cs.typed_count..];

            if !typed.is_empty() {
                spans.push(Span::styled(typed, Style::default().fg(Color::Green)));
            }
            cursor_pos += typed.len();

            if let Some(next) = remaining.chars().next() {
                // カーソル (白または赤)
                let style = if app_state.is_error {
                    Style::default().fg(Color::White).bg(Color::Red)
                } else {
                    Style::default().fg(Color::Black).bg(Color::White)
                };
                spans.push(Span::styled(next.to_string(), style));

                // カーソル以降の残り (灰色)
                if remaining.len() > 1 {
                    spans.push(Span::styled(
                        &remaining[1..],
                        Style::default().fg(Color::Gray),
                    ));
                }
            }
        } else {
            // まだ手をつけていない CharState (灰色)
            spans.push(Span::styled(pattern, Style::default().fg(Color::DarkGray)));
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)), chunks[4]);
    // カーソル位置を計算してセット
    f.set_cursor_position((chunks[4].x + cursor_pos as u16, chunks[4].y));
}
