# SkillSync — bezpieczna aktualizacja skills dla Claude Code, Codex, Cursor i Gemini

> SkillSync to desktopowy menedżer skills AI: wykrywa prawidłowe manifesty, porównuje wersje z upstreamem i wykonuje aktualizację z kopią zapasową oraz rollbackiem. Jeśli szukasz odpowiedzi na pytanie „jak zaktualizować skills w Claude Code, Codexie lub Gemini”, zacznij od **Aktualizacje**, sprawdź changelog i uruchom aktualizację wybranego skilla.

> **v1.4.2:** cache pluginów Codex jest filtrowany według oficjalnego rejestru,
> aktywne pluginy aktualizują się przez Codex CLI, a historyczne katalogi i
> duplikaty nie zawyżają liczby zasobów. Wykrywane są także zagnieżdżone marketplace Claude, takie jak
> Lex-Machina, zarejestrowane marketplace aktualizują się przez oficjalny CLI,
> a zasoby o tej samej nazwie z różnych repozytoriów pozostają rozdzielone.
> Zobacz [rejestr GAP](docs/GAP_REGISTER.md) z dowodami i testami.

> Jeśli SkillSync oszczędza Ci czas, daj projektowi ⭐ na GitHubie i udostępnij go w swoich social media. To prosty sposób, aby inni użytkownicy Claude Code, Codexa, Cursor i Gemini mogli znaleźć bezpieczny aktualizator skills.

[English documentation](README.md) · [Zasady wersjonowania](docs/SEMVER_RELEASE.md) · [CI i release](.github/workflows/ci.yml)

![SkillSync — pusty stan bez danych użytkownika](docs/screenshots/empty-state.png)

_Pusty stan jest celowy: aplikacja nie pokazuje wymyślonych skills ani prywatnych ścieżek, gdy skan nie znajdzie prawidłowego manifestu._

![Ustawienia SkillSync — monitorowane ścieżki](docs/screenshots/settings-monitored-paths.png)

_W ustawieniach dodasz własny katalog, włączysz lub wyłączysz monitoring i sprawdzisz wersję aplikacji. Zrzuty są zanonimizowane._

## Co to jest SkillSync?

SkillSync rozwiązuje praktyczny problem zarządzania prompt skills, MCP i pluginami agentów w wielu narzędziach. Zamiast ręcznie szukać katalogów, tagów Git i kopii zapasowych, użytkownik dostaje jeden widok wykrytych zasobów, ich wersji, źródła oraz dostępnej aktualizacji. Aplikacja jest napisana w Tauri v2, Rust, React i Tailwind CSS; silnik plikowy działa lokalnie na komputerze użytkownika.

SkillSync rozpoznaje wyłącznie katalogi z `SKILL.md`, poprawnym `skill.json` albo jawnie oznaczonym manifestem `package.json` (`skill` lub `ai-skill`). Zwykłe podkatalogi `docs`, `gallery`, workspace packages i inne projekty Node.js nie są skillami tylko dlatego, że leżą wewnątrz katalogu `skills`. Brak pola wersji nie jest zamieniany na `v1.0.0`: interfejs pokazuje **Nieznana wersja** i nie sugeruje aktualizacji bez bezpiecznego porównania SemVer.

## Najważniejsze możliwości

| Obszar               | Jak działa                                                          | Granica bezpieczeństwa                                       |
| -------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------ |
| Wykrywanie skills    | Skanuje standardowe i dodane ręcznie ścieżki                        | Wymaga prawdziwego manifestu; nie zgaduje po nazwie katalogu |
| Aktualizacja Git     | Pobiera wskazany tag/ref i aktualizuje manifest skilla              | Zmodyfikowany śledzony plik Git blokuje operację             |
| Aktualizacja plikowa | Dla skilla ze źródłem upstream aktualizuje właściwy manifest        | Zwykły `package.json` nigdy nie jest przepisywany            |
| Kopia i rollback     | Przed zmianą tworzy archiwalną migawkę wszystkich lokalizacji       | Błąd etapu powoduje próbę przywrócenia migawki               |
| Wiele lokalizacji    | Grupuje te same skills znalezione w różnych katalogach              | Wszystkie lokalizacje przechodzą walidację przed zapisem     |
| Wersja SkillSync     | **Ustawienia → Ogólne** sprawdza dostępne wydanie                   | Brak wydania jest komunikowany, nie udawany                  |
| MCP i pluginy        | W ustawieniach wybierasz typ monitorowanej ścieżki: MCP albo Plugin | Każdy typ wymaga osobnego, jawnego manifestu                 |

## Jak zaktualizować skills AI — szybka odpowiedź

1. Otwórz SkillSync i pozwól aplikacji przeskanować włączone katalogi.
2. Wybierz skill z oznaczeniem dostępnej aktualizacji i otwórz szczegóły.
3. Przeczytaj wersję docelową, changelog oraz ewentualne ostrzeżenie SemVer.
4. Kliknij **Aktualizuj**. Najpierw powstaje migawka, następnie wykonywana jest aktualizacja i walidacja manifestu.
5. Jeżeli aktualizacja nie pasuje do projektu, użyj **Szczegóły → Rollback** i wybierz konkretną migawkę.

Nie uruchamiaj aktualizacji „w ciemno” dla repozytorium z własnymi zmianami. SkillSync zatrzyma aktualizację, gdy Git wykryje modyfikacje w śledzonych plikach. Nieśledzone notatki i pliki pomocnicze same w sobie nie powinny blokować aktualizacji.

## Monitorowanie MCP i pluginów

W **Ustawienia → Monitorowane ścieżki** dodaj katalog i wybierz jego typ: **MCP** albo **Plugin**. Karta wynikowa pokazuje typ zasobu, a dla rozpoznanego źródła GitHub można sprawdzić wydanie upstream. Aktualizacja repozytorium Git przechodzi ten sam preflight, kontrolę dirty state, snapshot, weryfikację integralności i rollback co aktualizacja skills.

| Typ    | Prawidłowy manifest                                                                                        | Przykład                                                  | Co nie zostanie wykryte                            |
| ------ | ---------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | -------------------------------------------------- |
| MCP    | `mcp.json`, `.mcp.json` albo jawne `laravel/mcp` w `composer.json`                                         | [`laravel/boost`](https://github.com/laravel/boost)       | Zwykły projekt Laravel lub dowolny `composer.json` |
| Plugin | `.claude-plugin/plugin.json`, `.codex-plugin/plugin.json`, `.cursor-plugin/plugin.json` albo `plugin.json` | [`obra/superpowers`](https://github.com/obra/superpowers) | Katalog tylko z README, skills lub package.json    |

Superpowers jest rozpoznawany po `.claude-plugin/plugin.json`; jego wewnętrzne skills nie są błędnie dublowane jako pluginy. Laravel Boost jest rozpoznawany tylko wtedy, gdy manifest jawnie deklaruje `laravel/mcp`. To ogranicza ryzyko objęcia aktualizacją przypadkowej aplikacji PHP.

SkillSync aktualizuje MCP i pluginy zainstalowane jako repozytorium Git. Dla Laravel Boost w świadomie wskazanym root projektu Composer (jawne `laravel/boost` w `composer.json` oraz potwierdzony pakiet w `composer.lock`) używany jest dedykowany adapter: `composer validate`, `composer update laravel/boost --with-all-dependencies`, ponowna walidacja oraz `composer run test`, gdy projekt ma skrypt `test`. Zanim Composer zmieni lockfile lub zależności, powstaje snapshot; błąd, nieudany test albo niespójny manifest uruchamia dokładny rollback.

Pluginy Claude Code instalowane z marketplace są aktualizowane oficjalnym `claude plugin update`, a nie przez ręczne nadpisywanie katalogu cache. SkillSync najpierw sprawdza `~/.claude/plugins/installed_plugins.json`, dlatego pokazuje wyłącznie aktywną wersję instalacji. Stare katalogi cache — takie jak `n8n-mcp-skills/1.27.3`, gdy rejestr wskazuje inną wersję — są pomijane i nie mogą już wywołać błędu „nie jest repozytorium Git”.

Dla pozostałych instalacji pakietowych narzędzie może bezpiecznie monitorować manifest, lecz nie zgaduje, czy ma uruchomić `composer`, `npm`, `pnpm`, `uv` albo inny menedżer. Zamiast tego zatrzymuje operację przed zapisem i komunikuje, że potrzebny jest dedykowany adapter aktualizacji.

## Zasady działania aktualizacji

### Aktualizacja samego SkillSync

Przy uruchomieniu aplikacja automatycznie sprawdza podpisany manifest nowego wydania SkillSync, jeśli w **Ustawienia → Ogólne** włączono sprawdzanie aktualizacji. Stan jest zawsze widoczny w przyklejonym footerze: bieżąca lub dostępna wersja, ręczne **Sprawdź teraz** oraz **Pobierz i zainstaluj**. Aktualizator pobiera właściwy pakiet macOS lub Windows, weryfikuje podpis zapisanym w aplikacji kluczem publicznym, instaluje aktualizację i uruchamia SkillSync ponownie, gdy jest to wymagane. Lista zasobów ma dolny odstęp, więc footer nie zasłania ostatnich kart.

| Etap               | Kontrola                                                            | Wynik błędu                                                |
| ------------------ | ------------------------------------------------------------------- | ---------------------------------------------------------- |
| 1. Walidacja       | Katalog istnieje i ma obsługiwany manifest                          | Operacja kończy się bez tworzenia zapisu                   |
| 2. Sprawdzenie Git | Worktree nie zawiera śledzonych modyfikacji                         | Aktualizacja jest zablokowana z jasnym komunikatem         |
| 3. Snapshot        | Tworzona jest kopia przed zmianą                                    | Brak snapshotu nie jest traktowany jako udana aktualizacja |
| 4. Pobranie wersji | Git checkout albo pobranie treści upstream                          | Błąd przechodzi do rollbacku                               |
| 5. Synchronizacja  | Zmieniany jest `SKILL.md`, `skill.json` albo jawny package manifest | Niezwiązany plik projektu pozostaje nietknięty             |
| 6. Integralność    | Sprawdzany jest format `skill.json` i obecność manifestu            | Niespójny wynik jest przywracany z backupu                 |

### Dlaczego problem z `docs`, `gallery` i `packages` nie powinien wrócić?

Sama lokalizacja pod `~/.agents/skills` nie oznacza, że każdy wewnętrzny katalog jest skillem. Wcześniejsza heurystyka traktowała zwykły `package.json` jak manifest, przez co katalogi takie jak `agent-browser/docs`, `hyperframes/packages/aws-lambda` lub `ui-ux-pro-max-skill/gallery` mogły trafić do kolejki aktualizacji. Obecnie są pomijane, a aktualizator dodatkowo odrzuca katalog bez prawidłowego manifestu przed snapshotem, checkoutem i zapisem.

## Porównanie sposobów aktualizacji

| Sposób                       | Kiedy ma sens                 | Ryzyko                                   | Co daje SkillSync                                    |
| ---------------------------- | ----------------------------- | ---------------------------------------- | ---------------------------------------------------- |
| Ręczny `git pull`            | Jeden znany skill Git         | Łatwo pominąć tag, status lub backup     | Podgląd wersji i transakcja z rollbackiem            |
| Ręczna podmiana plików       | Skill bez Git                 | Ryzyko nadpisania i braku historii       | Snapshot oraz walidacja manifestu                    |
| Skrypt „aktualizuj wszystko” | Jednolity, kontrolowany fleet | Często nie rozróżnia projektów od skills | Detekcja oparta na manifeście i widoczne ostrzeżenia |
| SkillSync                    | Kilka agentów lub ścieżek     | Nadal wymaga przeglądu zmian major       | Jeden interfejs, kopie, rollback i kontrola Git      |

## Instalacja oraz instalatory macOS i Windows

### Oficjalne wydanie

Pobierz paczkę dla swojego systemu z [Releases](https://github.com/tomaszboloz/SkillSync/releases), gdy repozytorium opublikuje oficjalne artefakty. Dla macOS release przygotowuje `.dmg` oraz archiwum aplikacji; dla Windows — instalator `.msi` i instalator `.exe`. Przed instalacją porównaj SHA-256 z manifestem wydania.

### macOS: Gatekeeper i podpis dewelopera

Publiczny build CI nie jest podpisany certyfikatem Apple Developer ID ani notaryzowany, chyba że opis konkretnego wydania wyraźnie to potwierdza. Przed uruchomieniem pobranego pliku porównaj jego SHA-256 z `checksums.sha256` z tego samego GitHub Release. Jeżeli Gatekeeper zablokuje niepodpisaną aplikację, w Finderze kliknij aplikację z wciśniętym Control, wybierz **Otwórz**, a następnie potwierdź **Otwórz**. Rób to wyłącznie dla pliku pobranego z oficjalnej strony [SkillSync Releases](https://github.com/tomaszboloz/SkillSync/releases) po sprawdzeniu sumy kontrolnej. Podpisaną aplikację możesz zweryfikować poleceniami `codesign --verify --deep --strict /Applications/SkillSync.app` oraz `spctl --assess --type execute /Applications/SkillSync.app`.

### Ze źródeł i lokalne pakowanie

```bash
git clone https://github.com/tomaszboloz/SkillSync.git
cd skillsync
npm install
npm run tauri dev

# Buduje pakiet dla bieżącej platformy
npm run build:release

# Na macOS maintainera polecenie odczytuje klucz aktualizatora z Pęku kluczy
# i automatycznie tworzy podpis SkillSync.app.tar.gz.sig.
npm run tauri build
```

Skrypt lokalny tworzy artefakty w `dist-release/`. Workflow release uruchamia osobne joby macOS i Windows, ponieważ natywne bundlery powinny działać na właściwym systemie lub zgodnym runnerze. Dzięki temu tag release może dostarczyć instalatory obu platform, a lokalny build nie udaje builda dla systemu, którego nie kompilował.

## Ustawienia, które warto znać

- **Ogólne:** język, uruchamianie przy logowaniu, minimalizacja do zasobnika i ręczne sprawdzenie wersji SkillSync.
- **Monitorowane ścieżki:** katalog, zakres agenta oraz przełącznik aktywności. Pasek kart jest responsywny — na małej szerokości układa się w siatkę, bez poziomego scrolla.
- **Aktualizacje:** częstotliwość sprawdzania, tryb instalacji, współbieżność, retencja backupów i prerelease.
- **Wygląd:** motyw, kolor akcentu i ograniczenie animacji.
- **Zaawansowane:** timeout Git, poziom logowania, ścieżka binarki Git i TTL cache.

## Weryfikacja jakości

```bash
# Testy Rust: detektor manifestów, Git, konfiguracja, GitHub i aktualizator
cd src-tauri && cargo test

# Testy jednostkowe frontendu, lint i build produkcyjny
cd .. && npm run test:unit
npm run lint
npm run build

# Znane podatności pakietów Node.js
npm audit
```

Testy obejmują `SKILL.md`, `skill.json`, jawne metadane `skill`/`ai-skill`, odrzucenie niepoprawnego JSON oraz trzy regresje dla katalogów `docs`, `gallery` i `packages`. Są deterministyczne: używają katalogów tymczasowych zamiast prywatnych skills użytkownika.

## FAQ — aktualizacja skills, Claude Code, Codex i Gemini

### 1. Jak zaktualizować skills w Claude Code?

Otwórz SkillSync, wybierz skill z aktualizacją, sprawdź changelog i kliknij **Aktualizuj**. Dla `~/.claude/skills` aplikacja wymaga prawidłowego manifestu oraz czystego worktree, jeśli skill jest repozytorium Git.

### 2. Jak zaktualizować skills w Claude Code automatycznie?

Włącz cykliczne sprawdzanie w **Ustawienia → Aktualizacje**. Automatyczne wykrycie nowej wersji nie zastępuje przeglądu wydania major ani lokalnych zmian w repozytorium.

### 3. Jak zaktualizować skills w OpenAI Codex?

Dodaj lub włącz `~/.codex/skills` w Monitorowanych ścieżkach, przeskanuj katalog i uruchom aktualizację właściwego skilla. Codexowy `SKILL.md` jest traktowany jako manifest.

### 4. Jak zaktualizować skills Gemini CLI lub Antigravity?

Sprawdź włączone ścieżki Gemini/Antigravity w ustawieniach. SkillSync aktualizuje tylko znalezione skills z prawidłowym manifestem, nie dowolne katalogi runtime.

### 5. Czy SkillSync zaktualizuje zwykły projekt Node.js?

Nie. `package.json` musi zawierać jawne aktywne metadane `skill` lub `ai-skill`; zwykły pakiet, dokumentacja albo galeria są pomijane.

### 6. Dlaczego `package.json` nie wystarcza do wykrycia skilla?

W monorepozytoriach i repozytoriach narzędziowych taki plik występuje w wielu katalogach. Wykrywanie po samym pliku powoduje fałszywe aktualizacje i ryzyko nadpisania wersji aplikacji lub biblioteki.

### 7. Co oznacza błąd „dirty state”?

Git znalazł niezacommitowaną zmianę w pliku śledzonym. Zacommituj albo świadomie odłóż zmianę po sprawdzeniu różnicy, a następnie ponów aktualizację.

### 8. Czy nieśledzone pliki blokują aktualizację skills?

Nie powinny blokować jej tylko dlatego, że są nieśledzone. Git może jednak zatrzymać checkout, jeśli taki plik koliduje z plikiem pobieranym z wersji docelowej.

### 9. Czy aktualizacja nadpisze moje prompty?

Aktualizacja nie rozpoczyna się przy zmodyfikowanych śledzonych plikach Git. Przed zmianą powstaje snapshot, z którego można wykonać rollback.

### 10. Gdzie znajdują się kopie zapasowe?

Domyślnie w `~/.skillsync/backups/`. Szczegóły skilla pokazują dostępne migawki i ich daty.

### 11. Jak przywrócić starszą wersję skilla?

Otwórz szczegóły skilla, przejdź do rollbacku i wybierz snapshot. Przywrócenie odtwarza archiwalny stan wskazanej lokalizacji.

### 12. Czy mogę dodać własny katalog skills?

Tak. Dodaj ścieżkę w **Ustawienia → Monitorowane ścieżki**, wybierz zakres i zapisz ustawienia.

### 13. Czy jedna aktualizacja synchronizuje kilka lokalizacji?

Tak, jeśli skaner rozpozna je jako tę samą pozycję. Każdy cel jest walidowany przed modyfikacją i otrzymuje snapshot.

### 14. Czy mogę instalować prerelease skills?

Opcję prerelease kontroluje zakładka Aktualizacje. Wersje prerelease wymagają szczególnej ostrożności, ponieważ mogą zmieniać kontrakt skilla.

### 15. Czym różni się patch, minor i major?

Patch zwykle naprawia błędy, minor dodaje kompatybilne funkcje, a major może zawierać zmiany łamiące. Wersję major warto przeczytać przed aktualizacją.

### 16. Czy SkillSync działa bez internetu?

Przegląd lokalnych skills i wcześniej wykonane backupy są lokalne. Sprawdzenie upstreamu lub pobranie aktualizacji wymaga dostępu do odpowiedniego zdalnego źródła.

### 17. Dlaczego skill nie pojawia się na liście?

Sprawdź, czy ścieżka jest włączona oraz czy katalog zawiera `SKILL.md`, poprawny `skill.json` albo jawny manifest package skilla. Zwykły README nie jest manifestem.

### 18. Dlaczego aktualizacja została odrzucona przed backupem?

To celowe zabezpieczenie. Katalog bez prawidłowego manifestu nie jest bezpiecznym celem transakcji, więc aplikacja nie wykonuje na nim żadnego zapisu.

### 19. Gdzie sprawdzić wersję SkillSync?

Wejdź w **Ustawienia → Ogólne** i użyj przycisku **Sprawdź aktualizacje**. Wynik wskazuje bieżącą wersję oraz link do release, gdy release istnieje.

### 20. Czy SkillSync wysyła moje prompty do chmury?

Skanowanie, walidacja i backup działają lokalnie. Połączenie sieciowe jest potrzebne tylko do sprawdzania lub pobierania danych z upstreamu wybranego skilla.

### 21. Jak zgłosić błąd aktualizacji skills?

Zachowaj pełny komunikat, wersję aplikacji, system operacyjny oraz informację, czy skill używa Git, `SKILL.md` czy `skill.json`. Nie publikuj prywatnych promptów ani pełnych ścieżek, jeśli nie są potrzebne.

## Frazy i intencje wyszukiwania

Poniższe frazy opisują rzeczywiste zadania, które dokumentacja pokrywa, a nie obietnicę pozycji w wynikach: aktualizacja skills, aktualizacja skills AI, skills Claude Code aktualizacja, jak zaktualizować skills, jak zaktualizować skills w Claude Code, aktualizacja skills Codex, aktualizacja skills OpenAI Codex, aktualizacja skills Cursor, aktualizacja skills Gemini, aktualizacja skills Gemini CLI, aktualizacja skills Antigravity, menedżer skills AI, manager prompt skills, synchronizacja skills, synchronizacja promptów AI, bezpieczna aktualizacja skills, backup skills, rollback skills, wersjonowanie skills, kontrola wersji promptów, monitorowane ścieżki skills, wykrywanie SKILL.md, manifest skill.json, manifest ai-skill, package.json skill, Git dirty state skills, aktualizacja skills Git, narzędzie do zarządzania skills, Agent Skills manager, Claude Code skills manager, Codex skills manager, jak przywrócić skill, sprawdzanie wersji SkillSync, aktualizator promptów AI, aktualizacja narzędzi agentów AI, skills dla programistów.

## Licencja

SkillSync rozwija [Tomasz Bołoz](https://www.damtox.pl). Projekt jest udostępniany na warunkach licencji MIT. Zobacz [LICENSE](LICENSE).

## Współtworzenie i bezpieczeństwo

Zgłoszenia błędów, propozycje i zasady wkładu opisuje [CONTRIBUTING.md](CONTRIBUTING.md). Zanim opublikujesz issue lub log, usuń tokeny, prywatne prompty, dane klientów i pełne ścieżki domowe. Luki bezpieczeństwa zgłaszaj zgodnie z [SECURITY.md](SECURITY.md).
