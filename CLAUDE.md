# ruspell

cspell 호환 고속 Rust 코드 스펠체커

## 구조
- `crates/ruspell-core/` — 단어 분리, 검증 파이프라인
- `crates/ruspell-dict/` — 사전 로딩/조회
- `crates/ruspell-config/` — cspell.json 파싱
- `crates/ruspell-cli/` — CLI

## 빌드/테스트
```
cargo build
cargo test
cargo run -- check .
```

## 코드 스타일
- `thiserror`로 에러 타입 정의
- `hashbrown` + `foldhash` 사용
- 테스트에서만 `.unwrap()` 허용
- `rayon`으로 병렬 처리

## cspell 호환
- 동일한 설정 형식 (cspell.json)
- 동일한 사전 형식 (txt, trie)
- 동일한 인라인 지시자
- 동일한 단어 분리 동작
