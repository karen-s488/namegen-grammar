# namegen

A command-line tool that generates random names from a small text grammar.

## Problem

Most "random name generator" tools are either a hardcoded list baked into
the binary or a pile of JavaScript on some website. I wanted something I
could point at a plain text file describing how names in a given setting
are built out of syllables, and get a batch of names back - and when the
grammar file has a typo, get a real error with a line and column number
pointing at it, instead of a panic or a silently empty result.

## Grammar format

A grammar file is a list of rules, one per line:

```
rule_name = alternative1 | alternative2 | alternative3
```

Each alternative is plain text that can reference another rule with
`<rule_name>`. References are expanded recursively, and each expansion
picks one alternative at random. Lines starting with `#` are comments;
blank lines are ignored.

An alternative can also contain a `[...]` character class, which expands
to a single random character from the listed set. Ranges like `a-z` are
allowed inside the brackets:

```
initial = [A-Z]
digit = [0-9]
vowel = [aeiou]
```

A `-` at the very start or end of the brackets is a literal dash rather
than a range, e.g. `[a-z-]` matches a lowercase letter or a dash.

An alternative can end in `:N` to make it N times as likely to be picked
as a plain alternative, which otherwise has a weight of 1:

```
size = small:5 | medium:3 | large | enormous:1
```

Here `small` is picked five times as often as `large`, and `large` and
`enormous` are equally likely.

Example (`examples/fantasy.namegen`):

```
name = <syllable><syllable> | <syllable><syllable><syllable> | <syllable>'<syllable>

syllable = ka|ke|ki|ko|ku|ra|re|ri|ro|ru|sha|shi|tha|thi|zar|zan|dun|dan|mor|mir|fen|fal|bry|gor|nal|vess
```

Generation starts from the rule named `name` unless `--start` says
otherwise.

## Usage

```
cargo run -- examples/fantasy.namegen --count 10
```

```
namegen <grammar-file> [--start <rule>] [--count <n>] [--seed <n>]

  <grammar-file>   path to a .namegen grammar file
  --start <rule>   rule to expand first (default: name)
  --count <n>      how many names to generate (default: 10)
  --seed <n>       fix the random seed for reproducible output
```

## Error messages

Parsing points at the exact character that caused the problem. Given a
grammar file with a typo, `<sylable>` instead of `<syllable>`, on line 3:

```
error: undefined rule 'sylable'
  --> 3:8
  |
3 | name = <sylable><syllable>
  |        ^
```

Duplicate rule names, unterminated `<...>` references, empty alternatives,
and missing `=` signs all report the same way: a message, a line:column,
and the offending line with a caret under the exact spot.

## Building

Standard library only, no dependencies:

```
cargo build --release
```

## Status

Early. The grammar format above, including weighted alternatives and
character classes, is what's implemented so far. Casing flags and a
`--unique` output filter are next - see the issues, or just read
`src/grammar.rs`.
