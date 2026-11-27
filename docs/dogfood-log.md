# Dogfood log

Problems met while building real languages with Langmeld, one entry each. The log decides
what gets built next.

## Entry template

```md
### YYYY-MM-DD: <short title>

- **Language:** <the language being built>
- **Task:** <what was being done>
- **Problem:** <what got in the way>
- **Workaround:** <what was done instead, or "none">
- **Status:** open | fixed in <version> | will not fix
```

## Entries

### 2026-11-27: a line that ends in `>` continues

- **Language:** the stress-test language (`tests/stress`)
- **Task:** end `type: List<Map<K, V>>` with the end of the line
- **Problem:** a line ending in a binary operator continues on the next line, and `>` is a
  comparison operator, so the closing `>>` of a generic list swallows the line ending
- **Workaround:** end such lines with `;`
- **Status:** open

### 2026-11-27: a choice between lists of strings needs many tokens

- **Language:** the stress-test language (`tests/stress`)
- **Task:** allow `tags: "a" "b"` next to `when: a * 2`, told apart by two tokens
- **Problem:** a string with holes is several tokens, so the second token of a list of
  strings is not enough to tell it from an expression that starts with a string
- **Workaround:** lists hold names, numbers and durations only
- **Status:** open

### 2026-11-27: a list value and an expression value cannot share a rule

- **Language:** the stress-test language (`tests/stress`)
- **Task:** `tags: fast stable beta` and `when: -a + -b` as the values of one kind of line
- **Problem:** after an expression, `-` can continue it or start the next value, so a list
  of expressions is refused (`LM-G010`)
- **Workaround:** a list holds plain words, and the line is either that list or one
  expression, told apart by the second token
- **Status:** will not fix

