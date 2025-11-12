/*
 * src/questions.rs
 * お題データを管理するモジュール
 */

// `pub` をつけて、他のファイル (main.rs) から見えるようにする
/*
 * src/questions.rs
 * (romaji -> hiragana に変更)
 */

// 構造体のフィールド名を変更
#[derive(Copy, Clone)]
pub struct Question {
    pub japanese: &'static str, // 表示用 (漢字混じり)
    pub hiragana: &'static str, // タイピング用 (ひらがな)
}

/// 問題リスト (ひらがなの文字数昇順)
pub const QUESTIONS_LIST: &'static [Question] = &[
    // 2文字
    Question { japanese: "猫", hiragana: "ねこ" },
    Question { japanese: "犬", hiragana: "いぬ" },
    Question { japanese: "空", hiragana: "そら" },
    
    // 3文字
    Question { japanese: "海", hiragana: "うみ" },
    Question { japanese: "山", hiragana: "やま" },
    Question { japanese: "川", hiragana: "かわ" },
    Question { japanese: "車", hiragana: "くるま" },
    
    // 4文字
    Question { japanese: "リンゴ", hiragana: "りんご" },
    Question { japanese: "ミカン", hiragana: "みかん" },
    Question { japanese: "電話", hiragana: "でんわ" },
    Question { japanese: "時計", hiragana: "とけい" },

    // 5文字
    Question { japanese: "こんにちは", hiragana: "こんにちは" },
    Question { japanese: "ありがとう", hiragana: "ありがとう" },
    Question { japanese: "さようなら", hiragana: "さようなら" },
    Question { japanese: "飛行機", hiragana: "ひこうき" },

    // 6文字
    Question { japanese: "図書館", hiragana: "としょかん" },
    Question { japanese: "新幹線", hiragana: "しんかんせん" },
    Question { japanese: "動物園", hiragana: "どうぶつえん" },

    // 7文字
    Question { japanese: "水族館", hiragana: "すいぞくかん" },
    Question { japanese: "遊園地", hiragana: "ゆうえんち" },

    // 8文字
    Question { japanese: "駐車場", hiragana: "ちゅうしゃじょう" },
    Question { japanese: "高速道路", hiragana: "こうそくどうろ" },
];