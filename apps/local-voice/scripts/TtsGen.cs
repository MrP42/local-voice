// Minimal WinRT text-to-speech generator for the M2 fixtures.
//
// Why this exists: the SAPI "Desktop" voices reachable from PowerShell mangle German
// umlauts ("Straße" comes out as "Strahe", "Köln" as "Khn"), which makes them useless
// for testing a German recogniser and unpleasant to listen to. The good voices
// (Katja, Stefan) are OneCore voices and are only exposed through the WinRT
// Windows.Media.SpeechSynthesis API.
//
// Windows PowerShell 5.1 cannot consume WinMD references, so we compile this against
// them with csc.exe instead. No admin rights, no registry changes, no machine-wide
// side effects.
//
// Build (see make-fixtures.ps1, which does this automatically):
//   csc /target:exe /out:TtsGen.exe /r:Windows.Foundation.winmd /r:Windows.Media.winmd
//       /r:Windows.Storage.winmd /r:System.Runtime.WindowsRuntime.dll TtsGen.cs
//
// Usage:
//   TtsGen.exe --list
//   TtsGen.exe "Katja" "out.wav" "Text to speak"

using System;
using System.IO;
using Windows.Media.SpeechSynthesis;
using Windows.Storage.Streams;

internal static class TtsGen
{
    private static int Main(string[] args)
    {
        try
        {
            if (args.Length == 1 && args[0] == "--list")
            {
                foreach (var v in SpeechSynthesizer.AllVoices)
                    Console.WriteLine("{0}|{1}|{2}", v.DisplayName, v.Language, v.Gender);
                return 0;
            }

            if (args.Length < 3)
            {
                Console.Error.WriteLine("usage: TtsGen.exe <voice-substring> <out.wav> <text>");
                Console.Error.WriteLine("       TtsGen.exe --list");
                return 2;
            }

            string voiceNeedle = args[0];
            string outPath     = args[1];
            string text        = string.Join(" ", args, 2, args.Length - 2);

            var synth = new SpeechSynthesizer();
            VoiceInformation picked = null;
            foreach (var v in SpeechSynthesizer.AllVoices)
            {
                if (v.DisplayName.IndexOf(voiceNeedle, StringComparison.OrdinalIgnoreCase) >= 0)
                {
                    picked = v;
                    break;
                }
            }
            if (picked == null)
            {
                Console.Error.WriteLine("voice not found: " + voiceNeedle);
                return 3;
            }
            synth.Voice = picked;

            // SynthesizeTextToStreamAsync yields a WAV stream; .AsTask() gives us a
            // normal Task so this stays a straightforward blocking console tool.
            var stream = synth.SynthesizeTextToStreamAsync(text).AsTask().GetAwaiter().GetResult();
            uint size = (uint)stream.Size;

            var reader = new DataReader(stream.GetInputStreamAt(0));
            reader.LoadAsync(size).AsTask().GetAwaiter().GetResult();
            var bytes = new byte[size];
            reader.ReadBytes(bytes);
            reader.Dispose();

            File.WriteAllBytes(outPath, bytes);
            Console.WriteLine("{0}|{1}|{2}", picked.DisplayName, outPath, bytes.Length);
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine("error: " + ex.Message);
            return 1;
        }
    }
}
