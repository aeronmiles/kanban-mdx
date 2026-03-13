//! Generates random two-word agent names like "quiet-storm".

use rand::{RngExt, rng};
use std::fs;
use std::sync::OnceLock;

/// System dictionary path.
const DICT_PATH: &str = "/usr/share/dict/words";

/// Number of words in a generated name.
const NAME_WORD_COUNT: usize = 2;

/// Generate a unique agent name from two random words joined with "-".
///
/// Tries the system dictionary first (`/usr/share/dict/words`), filtering for
/// 4-8 character lowercase ASCII words. Falls back to an embedded word list
/// if the dictionary is unavailable.
pub fn generate() -> String {
    let words = load_words();
    let mut rng = rng();

    let parts: Vec<&str> = (0..NAME_WORD_COUNT)
        .map(|_| {
            let idx = rng.random_range(0..words.len());
            words[idx].as_str()
        })
        .collect();

    parts.join("-")
}

/// Load the word list, preferring the system dictionary.
fn load_words() -> &'static Vec<String> {
    static WORDS: OnceLock<Vec<String>> = OnceLock::new();
    WORDS.get_or_init(|| {
        if let Some(dict_words) = read_dict_file() {
            if !dict_words.is_empty() {
                return dict_words;
            }
        }
        FALLBACK_WORDS.iter().map(|s| (*s).to_string()).collect()
    })
}

/// Read and filter the system dictionary file.
fn read_dict_file() -> Option<Vec<String>> {
    let content = fs::read_to_string(DICT_PATH).ok()?;
    let words: Vec<String> = content
        .lines()
        .filter(|w| is_valid_word(w))
        .map(|w| w.to_string())
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words)
    }
}

/// Check that a word is 4-8 lowercase ASCII letters only.
fn is_valid_word(w: &str) -> bool {
    let len = w.len();
    (4..=8).contains(&len) && w.bytes().all(|b| b.is_ascii_lowercase())
}

/// Embedded fallback words for when system dictionary isn't available.
const FALLBACK_WORDS: &[&str] = &[
    "able", "acid", "aged", "also", "area", "army", "away", "baby", "back", "ball",
    "band", "bank", "base", "bath", "bear", "beat", "been", "bell", "best", "bird",
    "blow", "blue", "boat", "body", "bomb", "bond", "bone", "book", "born", "boss",
    "both", "burn", "busy", "call", "calm", "came", "camp", "card", "care", "case",
    "cash", "cast", "cell", "chat", "chip", "city", "club", "coal", "coat", "code",
    "cold", "come", "cook", "cool", "cope", "copy", "core", "cost", "crew", "crop",
    "dark", "data", "date", "dawn", "dead", "deal", "dear", "deep", "deny", "desk",
    "dial", "diet", "dirt", "dish", "disk", "dock", "does", "done", "door", "dose",
    "down", "draw", "drew", "drop", "drug", "drum", "dual", "duke", "dull", "dust",
    "duty", "each", "earn", "ease", "east", "easy", "edge", "else", "even", "ever",
    "evil", "exam", "exit", "face", "fact", "fade", "fail", "fair", "fall", "fame",
    "farm", "fast", "fate", "fear", "feed", "feel", "feet", "fell", "felt", "file",
    "fill", "film", "find", "fine", "fire", "firm", "fish", "fist", "five", "flag",
    "flat", "fled", "flew", "flip", "flow", "folk", "food", "foot", "ford", "form",
    "fort", "foul", "four", "free", "from", "fuel", "full", "fund", "fury", "fuse",
    "gain", "game", "gang", "gate", "gave", "gaze", "gear", "gene", "gift", "girl",
    "give", "glad", "glow", "glue", "goat", "goes", "gold", "golf", "gone", "good",
    "grab", "gray", "grew", "grey", "grid", "grip", "grow", "gulf", "guru", "gust",
    "half", "hall", "halt", "hand", "hang", "hard", "harm", "hate", "have", "haul",
    "head", "heal", "heap", "hear", "heat", "held", "hell", "help", "here", "hero",
    "hide", "high", "hike", "hill", "hint", "hire", "hold", "hole", "holy", "home",
    "hood", "hook", "hope", "horn", "host", "hour", "huge", "hull", "hung", "hunt",
    "hurt", "icon", "idea", "inch", "into", "iron", "isle", "item", "jack", "jail",
    "jazz", "jean", "jolt", "jump", "jury", "just", "keen", "keep", "kept", "kick",
    "kind", "king", "kiss", "knee", "knew", "knit", "knot", "know", "lack", "laid",
    "lake", "lamp", "land", "lane", "last", "late", "lawn", "lead", "leaf", "lean",
    "left", "lend", "lens", "lent", "less", "lick", "life", "lift", "like", "limb",
    "lime", "limp", "line", "link", "lion", "list", "live", "load", "loan", "lock",
    "logo", "lone", "long", "look", "lord", "lose", "loss", "lost", "loud", "love",
    "luck", "lump", "lung", "lure", "lurk", "made", "mail", "main", "make", "male",
    "mall", "many", "mark", "mask", "mass", "mate", "maze", "meal", "mean", "meat",
    "meet", "melt", "memo", "mend", "menu", "mere", "mesh", "mild", "milk", "mill",
    "mind", "mine", "mint", "miss", "mist", "mode", "mold", "moon", "more", "moss",
    "most", "move", "much", "must", "myth", "nail", "name", "navy", "near", "neat",
    "neck", "need", "nest", "news", "next", "nice", "nine", "node", "none", "norm",
    "nose", "note", "noun", "odds", "omit", "once", "only", "onto", "open", "oral",
    "oven", "over", "pace", "pack", "page", "paid", "pain", "pair", "pale", "palm",
    "pane", "park", "part", "pass", "past", "path", "peak", "peel", "peer", "pick",
    "pile", "pine", "pink", "pipe", "plan", "play", "plea", "plot", "ploy", "plug",
    "plus", "poem", "poet", "poll", "polo", "pond", "pool", "poor", "pope", "pork",
    "port", "pose", "post", "pour", "pray", "prey", "prop", "pull", "pump", "pure",
    "push", "quit", "quiz", "race", "rack", "rage", "raid", "rail", "rain", "rank",
    "rare", "rate", "read", "real", "rear", "reef", "rein", "rely", "rent", "rest",
    "rice", "rich", "ride", "rift", "ring", "rise", "risk", "road", "roam", "rock",
    "rode", "role", "roll", "roof", "room", "root", "rope", "rose", "ruin", "rule",
    "rush", "safe", "sage", "said", "sail", "sake", "sale", "salt", "same", "sand",
    "sang", "save", "seal", "seat", "seed", "seek", "seem", "seen", "self", "sell",
    "send", "sent", "shed", "shin", "ship", "shop", "shot", "show", "shut", "sick",
    "side", "sigh", "sign", "silk", "sink", "site", "size", "skip", "slam", "slap",
    "slim", "slip", "slot", "slow", "snap", "snow", "soak", "soar", "sock", "soft",
    "soil", "sold", "sole", "some", "song", "soon", "sort", "soul", "spin", "spit",
    "spot", "star", "stay", "stem", "step", "stir", "stop", "stun", "such", "suit",
    "sure", "surf", "swim", "tail", "take", "tale", "talk", "tall", "tank", "tape",
    "task", "team", "tear", "teen", "tell", "tend", "tens", "tent", "term", "test",
    "text", "than", "that", "them", "then", "they", "thin", "this", "thus", "tick",
    "tide", "tidy", "tier", "tile", "till", "time", "tiny", "tire", "toad", "toil",
    "told", "toll", "tone", "took", "tool", "tops", "tore", "torn", "toss", "tour",
    "town", "trap", "tray", "tree", "trek", "trim", "trio", "trip", "true", "tube",
    "tuck", "tuna", "tune", "turn", "twin", "type", "ugly", "undo", "unit", "upon",
    "urge", "used", "user", "vain", "vale", "vary", "vast", "veil", "vein", "vent",
    "verb", "very", "vest", "veto", "vice", "view", "vine", "visa", "void", "volt",
    "vote", "wade", "wage", "wait", "wake", "walk", "wall", "want", "ward", "warm",
    "warn", "warp", "wash", "wave", "weak", "wear", "weed", "week", "well", "went",
    "were", "west", "what", "when", "whom", "wide", "wife", "wild", "will", "wind",
    "wine", "wing", "wire", "wise", "wish", "with", "woke", "wolf", "wood", "wool",
    "word", "wore", "work", "worm", "worn", "wrap", "yard", "yarn", "year", "yell",
    "yoga", "your", "zeal", "zero", "zinc", "zone", "zoom",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_format() {
        let name = generate();
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 2, "expected two words joined by '-', got: {name}");
        for part in &parts {
            assert!(!part.is_empty(), "word should not be empty in: {name}");
        }
    }

    #[test]
    fn test_generate_uniqueness() {
        // Generate several names and check they're not all the same
        let names: Vec<String> = (0..10).map(|_| generate()).collect();
        let unique: std::collections::HashSet<&String> = names.iter().collect();
        assert!(unique.len() > 1, "expected some variety in generated names");
    }

    #[test]
    fn test_is_valid_word() {
        assert!(is_valid_word("calm"));
        assert!(is_valid_word("storm"));
        assert!(is_valid_word("whisper")); // 7 chars
        assert!(is_valid_word("complete")); // 8 chars

        assert!(!is_valid_word("hi"));      // too short
        assert!(!is_valid_word("abc"));      // too short
        assert!(!is_valid_word("abcdefghi")); // too long (9)
        assert!(!is_valid_word("Hello"));    // uppercase
        assert!(!is_valid_word("it's"));     // punctuation
        assert!(!is_valid_word("123four"));  // digits
    }

    #[test]
    fn test_fallback_words_all_valid() {
        for word in FALLBACK_WORDS {
            assert!(
                is_valid_word(word),
                "fallback word {word:?} should be valid"
            );
        }
    }

    #[test]
    fn test_load_words_returns_nonempty() {
        let words = load_words();
        assert!(!words.is_empty());
    }
}
