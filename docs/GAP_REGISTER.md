# SkillSync — rejestr GAP

Ten rejestr opisuje zakres audytu jakościowego wykonany dla wydania po `v1.3.1`.
Wpis oznaczony `RESOLVED` ma implementację w kodzie oraz test regresyjny albo
istniejącą bramkę CI. Rejestr nie jest obietnicą pokrycia przyszłych zmian
upstreamu ani nie zastępuje testów środowiskowych na macOS i Windows.

## Wykrywanie i źródła aktualizacji

| ID | GAP | Status | Dowód |
| --- | --- | --- | --- |
| GAP-001 | Głęboki układ marketplace (`.claude/skills`) był poza limitem skanowania | RESOLVED | `SkillDetector`, test `discovers_skills_nested_in_a_claude_marketplace_repository` |
| GAP-002 | Zagnieżdżone repozytorium nie dziedziczyło remote origin | RESOLVED | `GitService::get_remote_url`, test `resolves_remote_for_a_skill_nested_inside_its_git_repository` |
| GAP-003 | Dzieci manifestu mogły tworzyć fałszywe karty | RESOLVED | granica `skip_current_dir`, testy detectora/managed detectora |
| GAP-004 | `node_modules`, `target` i `dist` zwiększały szum skanowania | RESOLVED | filtr katalogów detectorów |
| GAP-005 | Zwykły `package.json` był mylony ze skillem | RESOLVED | `SkillManifest::is_explicit_package_skill`, test `rejects_ordinary_package_json` |
| GAP-006 | Uszkodzony `skill.json` mógł być maskowany przez `SKILL.md` | RESOLVED | walidacja manifestu, test `rejects_malformed_skill_json` |
| GAP-007 | Brak wersji był prezentowany jako realny release | RESOLVED | stała `UNKNOWN_SKILL_VERSION`, testy wersji |
| GAP-008 | Prefix `v` powodował rozbieżność po aktualizacji | RESOLVED | normalizacja w `version_from_manifest` i detectorze |
| GAP-009 | Remote SSH i HTTPS nie miały wspólnego formatu | RESOLVED | `GitHubService::normalize_github_repository_url` |
| GAP-010 | URL gałęzi GitHub był zapisywany jako repozytorium | RESOLVED | walidacja root URL, test `normalizes_only_github_repository_roots` |
| GAP-011 | Marketplace bez `.git` nie miał wykrywalnego źródła | RESOLVED | odczyt `known_marketplaces.json` |
| GAP-012 | Plugin marketplace nie był odróżniany od cache pluginu | RESOLVED | `ClaudePluginService::marketplace_for_path` |
| GAP-013 | Stare cache pluginów Claude tworzyły fałszywe aktualizacje | RESOLVED | weryfikacja `installed_plugins.json` |
| GAP-014 | Repo bez tagów SemVer nie miało bezpiecznego fallbacku | RESOLVED | `resolve_fallback_branch` |
| GAP-015 | Ręczna gałąź mogła przyjąć URL lub znaki refspec | RESOLVED | `is_valid_branch_name` + testy negatywne |
| GAP-016 | Auto-detected branch nadpisywał politykę tagów | RESOLVED | override działa tylko jawnie |
| GAP-017 | `main` i branch domyślny nie były sprawdzane upstream | RESOLVED | `ls-remote --heads` przed checkoutem |
| GAP-018 | Najnowszy tag nie był wybierany deterministycznie | RESOLVED | najwyższy SemVer z `ls-remote` |
| GAP-019 | Pre-release nie wygrywał ze stabilnym releasem | RESOLVED | test `stable_release_is_not_older_than_its_prerelease` |
| GAP-020 | Detektor pluginów/MCP był zależny od nazwy katalogu | RESOLVED | jawne manifesty `ManagedManifest` |

## Transakcje, integralność i rollback

| ID | GAP | Status | Dowód |
| --- | --- | --- | --- |
| GAP-021 | Brak lokalizacji mógł zostać pominięty przed snapshotem | RESOLVED | `resolve_locations`, test niedostępnej ścieżki |
| GAP-022 | Dwa zasoby z jednego repo mogły wykonać dwa checkouty | RESOLVED | `plan_operations`, test nested operation |
| GAP-023 | Bulk update mógł równolegle dotknąć jednego repo | RESOLVED | `UPDATE_TRANSACTION_LOCK` |
| GAP-024 | Każda lokalizacja nie miała własnego snapshotu | RESOLVED | unikalna nazwa z nanosekundami |
| GAP-025 | ID snapshotu kolidowało w tej samej sekundzie | RESOLVED | ID snapshotu z nanosekundami + test |
| GAP-026 | Snapshot podążał za symlinkiem poza źródło | RESOLVED | `tar.follow_symlinks(false)` |
| GAP-027 | Dangling symlink blokował backup | RESOLVED | test `snapshot_preserves_a_dangling_symlink_without_following_it` |
| GAP-028 | Rollback nie usuwał plików dodanych przez update | RESOLVED | czyszczenie katalogu przed unpack |
| GAP-029 | Rollback mógł celować w katalog domowy | RESOLVED | `validated_restore_target` |
| GAP-030 | Checkout dirty worktree był niejawnie destrukcyjny | RESOLVED | twarda bramka `WorktreeDirty` |
| GAP-031 | Untracked pliki blokowały legalny update | RESOLVED | status tracked-only + ochrona kolizji |
| GAP-032 | Konflikt untracked/tracked symlink kończył checkout | RESOLVED | `preserve_checkout_conflicts` |
| GAP-033 | Wiele lokalizacji wybierało tag osobno | RESOLVED | plan tagów per remote przed snapshotem |
| GAP-034 | Różne tagi upstream mogły dać częściowy update | RESOLVED | porównanie `resolved_tag` i rollback |
| GAP-035 | Prefix wersji mógł dać fałszywy mismatch końcowy | RESOLVED | normalizacja `observed_versions` |
| GAP-036 | Branch checkout nie odczytywał wersji manifestu | RESOLVED | `version_from_manifest` po checkout |
| GAP-037 | Integrity gate nie obejmował wszystkich lokalizacji | RESOLVED | końcowa walidacja `canonical_targets` |
| GAP-038 | Raw skill mógł dostać nową wersję bez treści upstream | RESOLVED | błąd i rollback przy nieudanym fetchu |
| GAP-039 | Błąd zapisu manifestu był ignorowany | RESOLVED | propagacja błędu + rollback |
| GAP-040 | Usunięcie symlinku mogło usunąć target | RESOLVED | `remove_location` unlinkuje sam link |

## Adaptery właścicieli pakietów

| ID | GAP | Status | Dowód |
| --- | --- | --- | --- |
| GAP-041 | Laravel Boost był aktualizowany jak zwykły katalog | RESOLVED | jawny adapter Composer |
| GAP-042 | Composer update nie walidował projektu przed zmianą | RESOLVED | `composer validate` |
| GAP-043 | Composer update nie walidował lockfile po zmianie | RESOLVED | druga walidacja + wersja z locka |
| GAP-044 | Test projektu Composer nie był uruchamiany | RESOLVED | skrypt `test`, jeśli jawnie istnieje |
| GAP-045 | Cache Claude był zmieniany bez rejestru | RESOLVED | aktywny wpis `installed_plugins.json` |
| GAP-046 | Brak Claude CLI kończył się nieczytelnym ENOENT | RESOLVED | resolver PATH/home + komunikat |
| GAP-047 | PATH Node/NVM nie był dostępny z GUI | RESOLVED | `runtime_path` |
| GAP-048 | Marketplace Claude był zgłaszany jako „nie jest Git” | RESOLVED | adapter `plugin marketplace update` |
| GAP-049 | Marketplace nie miał bramki manifestu po update | RESOLVED | ponowny odczyt rejestru i manifestu |
| GAP-050 | Aktualizacja pluginu nie sprawdzała aktywnych lokalizacji | RESOLVED | `active_locations` + manifest validation |

## Sieć, kolejka, bezpieczeństwo i wydanie

| ID | GAP | Status | Dowód |
| --- | --- | --- | --- |
| GAP-051 | Masowe skanowanie odpytywało upstream równolegle | RESOLVED | `ScanQueue` jeden worker |
| GAP-052 | Brak odstępu między żądaniami GitHub | RESOLVED | `REQUEST_INTERVAL` |
| GAP-053 | Stare wyniki skanu mogły nadpisać nowe | RESOLVED | generacja kolejki |
| GAP-054 | Ten sam remote był sprawdzany wielokrotnie | RESOLVED | `upstream_cache_key` |
| GAP-055 | Detail check i scan miały różną semantykę release | RESOLVED | wspólne `refresh_upstream` |
| GAP-056 | Pojedynczy błąd sieci kończył check | RESOLVED | retry fallback chain |
| GAP-057 | Odpowiedź GitHub bez taga była uznawana za release | RESOLVED | walidacja JSON/Atom/redirect |
| GAP-058 | Nieznana wersja mogła wygenerować update | RESOLVED | SemVer-only comparison |
| GAP-059 | URL Windows przechodził przez shell `cmd` | RESOLVED | bezpośredni `explorer.exe` |
| GAP-060 | `open_url` akceptował dowolny schemat | RESOLVED | tylko HTTP(S), test negatywny |
| GAP-061 | Konfiguracja była zapisywana nieatomowo | RESOLVED | plik tymczasowy + `sync_all` + rename |
| GAP-062 | Tauri updater nie miał wymuszonego podpisu w CI | RESOLVED | secret-gated signing step |
| GAP-063 | Build macOS bez certyfikatu był blokowany przez puste env | RESOLVED | rozdzielone kroki unsigned/signed |
| GAP-064 | Release nie sprawdzał obu platform | RESOLVED | wymagane DMG, MSI i EXE |
| GAP-065 | Manifest updatera mógł wskazywać nieistniejący artefakt | RESOLVED | walidacja targetów i sygnatur |
| GAP-066 | Brakowało testu dla nested marketplace discovery | RESOLVED | regresja Lex-Machina |
| GAP-067 | Brakowało testu dla registered marketplace source | RESOLVED | regresja n8n marketplace |
| GAP-068 | Błąd aktualizacji mógł zmienić lokalny manifest mimo braku upstream | RESOLVED | regresja non-Git raw skill |
| GAP-069 | Branch ref przyjmował niebezpieczne separatory | RESOLVED | testy `//`, `/feature`, URL |
| GAP-070 | Zmiana wersji nie miała jawnego zakresu rollbacku | RESOLVED | snapshoty planowane przed mutacją |

## Codex registry and resource accounting

| ID | GAP | Status | Dowód |
| --- | --- | --- | --- |
| GAP-071 | Historyczny Codex cache był traktowany jak repozytorium Git | RESOLVED | `CodexPluginService`, filtr `~/.codex/plugins/cache` |
| GAP-072 | Aktualizacje Codex pluginów nie miały właścicielskiego adaptera | RESOLVED | `codex plugin marketplace upgrade` + `codex plugin add`, test JSON registry |
| GAP-073 | Katalog marketplace zawierał niezainstalowane pluginy liczone jako zasoby | RESOLVED | filtr aktywnych ścieżek z `codex plugin list --available --json` |
| GAP-074 | Symlink/canonical path mógł zawyżać liczbę lokalizacji | RESOLVED | deduplikacja canonical path w detectorach |
| GAP-075 | Brakowało rozdzielenia identycznej nazwy od identycznego źródła | RESOLVED | klucz discovery `name/id + normalized source`, testy różnych repozytoriów |
| GAP-076 | Aktywny Codex cache z `SKILL.md` był dublowany przez ogólny skaner | RESOLVED | filtr właściciela Codex przed detekcją Skill, testy Rust |
| GAP-077 | Rejestr Codex był odczytywany wielokrotnie podczas jednego skanu | RESOLVED | snapshot rejestru ograniczony do monitorowanego drzewa |
| GAP-078 | Stare ścieżki hash-cache pozostawały w konfiguracji po migracji | RESOLVED | sanitizacja konfiguracji przy odczycie i zapisie |
| GAP-079 | Licznik zasobów mieszał zasoby logiczne z lokalizacjami fizycznymi | RESOLVED | deduplikacja ścieżki kanonicznej i jawny raport lokalizacji |
