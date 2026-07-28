# Recherchemethode

## Evidenzklassen

| Klasse | Definition | Notation |
|---|---|---|
| **OBSERVED** | Auf einer selbst abgerufenen Seite belegt | Aussage + URL + Abrufdatum |
| **SOURCE-CLAIM** | Anbieter oder Dritter behauptet es, nicht verifizierbar | Aussage + wer behauptet |
| **INFERRED** | Eigene Schlussfolgerung aus OBSERVED-Fakten | Aussage + woraus geschlossen |
| **UNKNOWN** | Öffentlich nicht feststellbar | benennen, nicht füllen |

Die Klasse gehört an die einzelne Aussage, nicht an das Dokument. Ein Absatz enthält oft alle vier.

**Der teuerste Fehler ist, SOURCE-CLAIM als OBSERVED zu führen.** „Die Verarbeitung erfolgt
lokal" auf einer Marketingseite ist eine Behauptung, keine Beobachtung — auch wenn sie
prominent steht.

## Quellenhierarchie

1. **Primär, verifizierbar** — Repository am Commit, offizielle API-/Produktdoku, Gesetzestext,
   Lizenzdatei, veröffentlichte Benchmark-Tabelle mit Methodik
2. **Primär, interessengeleitet** — Herstellerseite, Datenblatt, Pressemitteilung.
   Faktisch oft korrekt, in der Auswahl aber selektiv
3. **Sekundär, unabhängig** — Fachpresse, begutachtete Arbeiten, unabhängige Tests
4. **Sekundär, interessengeleitet** — Konkurrenzvergleiche, Affiliate-Blogs, SEO-Inhalte.
   Verwendbar als **Hinweis**, nie als Beleg
5. **Anekdotisch** — Foren, Rezensionen, Issues. Gut für Fehlermodi und reale Nutzung

Prüfe bei Dritten immer die Interessenlage: Impressum, Affiliate-Hinweis, ob der Autor ein
Konkurrenzprodukt betreibt. Ein Vergleich, den ein Wettbewerber geschrieben hat, ist keine
neutrale Quelle — auch wenn einzelne Fakten darin stimmen.

## Umgang mit Widersprüchen

Wenn Quellen sich widersprechen, **löse den Widerspruch nicht durch Mitteln auf.** Führe beide
Werte mit Quelle auf und benenne, welche verbindlicher ist. Beispiel: eine AGB ist verbindlicher
als eine FAQ, und beide sind verbindlicher als ein fremder Blogartikel.

Ein Widerspruch ist oft selbst ein Befund — er zeigt, wo ein Anbieter unpräzise ist.

## Negative Befunde sind Befunde

„Keine unabhängige Rezension auffindbar", „kein Changelog vorhanden", „Systemanforderungen
nirgends veröffentlicht" sind wertvolle Ergebnisse. Notiere ausdrücklich, **wo** du gesucht hast,
damit die Aussage überprüfbar ist. Ein negatives Ergebnis ohne Suchraum ist wertlos.

## Parallele Recherche

Unabhängige Recherchestränge (Produkt, OSS-Kandidaten, Technologie A, Technologie B) laufen gut
parallel. Nicht parallelisieren lassen sich: Entscheidungen, die auf mehreren Strängen aufbauen.

Gib jedem Rechercheauftrag mit: Ziel, Evidenzklassen-Pflicht, Verbot des Erfindens, gewünschtes
Ausgabeformat, und die ausdrückliche Erlaubnis, „nicht gefunden" zu melden. Ohne diese Erlaubnis
neigen Rechercheergebnisse zum Auffüllen von Lücken.

**Nimm Ergebnisse nicht ungeprüft an.** Prüfe entscheidungskritische Angaben — insbesondere
Lizenzen und Zahlen, auf denen die Architektur beruht — selbst nach.

## Abrufdatum

Jede Quelle bekommt ein Abrufdatum. Produktseiten, Preise und Nutzungsbedingungen ändern sich;
eine Aussage ohne Datum ist in drei Monaten wertlos. Bei schnelllebigen Themen (Anbieterpolitik,
Modellverfügbarkeit) zusätzlich vermerken, dass eine Neuprüfung vor Release nötig ist.
