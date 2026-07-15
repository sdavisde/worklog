# Changelog

## [1.3.0](https://github.com/sdavisde/worklog/compare/v1.2.0...v1.3.0) (2026-07-13)


### Features

* support wrapping task lines ([fe3b7ac](https://github.com/sdavisde/worklog/commit/fe3b7ac9ef8685cb2e4bc59b72031d2a89ccadb8))

## [1.2.0](https://github.com/sdavisde/worklog/compare/v1.1.0...v1.2.0) (2026-07-13)


### Features

* **standup:** merge today's completions into a unified section ([4250f3c](https://github.com/sdavisde/worklog/commit/4250f3c64b32b63d012e06836e8dc52f007df971))

## [1.1.0](https://github.com/sdavisde/worklog/compare/v1.0.0...v1.1.0) (2026-07-12)


### Features

* **tui:** add project editing, age, sort, and grouping to tasks ([21723ea](https://github.com/sdavisde/worklog/commit/21723eac0145197eb4c7beb98334bc585474d6c3))

## [1.0.0](https://github.com/sdavisde/worklog/compare/v0.3.0...v1.0.0) (2026-07-10)


### ⚠ BREAKING CHANGES

* **tui:** custom theme files under ~/.worklog/themes/ that declare insert_mode or normal_mode keys will fail to parse; remove those keys from any custom theme YAML.

### Features

* **notes:** add note deletion behind a shared confirm modal ([fd21201](https://github.com/sdavisde/worklog/commit/fd21201156a7ca0f917c72d9bbb84fa0b9d8eb6d))
* **tui:** replace vim-modal text editing with unified desktop-nav input ([c16444d](https://github.com/sdavisde/worklog/commit/c16444d66f8383963962a9909abe809d21aafada))
