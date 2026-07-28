# Fixture voice setup

The legacy SAPI "Desktop" voices reachable from Windows PowerShell 5.1 mispronounce
German umlauts badly. Measured on this machine with Hedda Desktop:

| written | spoken as | recognised as |
|---|---|---|
| Straße | "Strahe" | Strahe |
| großen | — | Kroan A |
| Köln | — | Khn / KALN |
| großartig | — | groyatisch |

That tests the synthesiser's defects, not the recogniser.

## Fix

PowerShell 7 was installed (user-scope, via winget) and `TtsGen` compiled against the
Windows SDK WinRT projection, which exposes the OneCore voices:

    Microsoft Stefan | de-DE | Male
    Microsoft Katja  | de-DE | Female
    Microsoft Hedda  | de-DE | Female

Katja pronounces German correctly. With Katja fixtures the same recogniser returns
proper German, e.g.

    Kommst du morgen mit. Das wäre wirklich großartig. Ich warte, bis du da bist.

## Regenerating

    apps/local-voice/scripts/bin/TtsGen.exe --list
    apps/local-voice/scripts/bin/TtsGen.exe Katja out.wav "Text"

Source: `apps/local-voice/scripts/TtsGen.cs`. Build needs an explicit NuGet source,
because none is configured on this machine:

    dotnet restore -s https://api.nuget.org/v3/index.json
    dotnet build -c Release --no-restore
