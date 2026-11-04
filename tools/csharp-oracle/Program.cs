// The C# oracle: Roslyn's own lexer and parser (Microsoft.CodeAnalysis.CSharp, pinned by packages.lock.json) over a
// list of files, printing what the Rust port (crates/csharp-syntax, `ksdump --lang csharp`) must print byte for byte.
// README.md is the normative description of every mode and of the dump format.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using System.Threading;

namespace CSharpOracle;

static class Program
{
    const string Usage = """
        usage: oracle.sh MODE ARGS
          dump --list LIST --root ROOT --out OUT [--version latest|preview|N] [--doc parse|none|diagnose]
               [--define A;B] [--entry NAME] [--threads N] [--stack-mb M]
          dump --serve [--version ...] [--doc ...] [--define ...] [--entry ...] [--threads N]
               (stdin: LIST<TAB>ROOT<TAB>OUT, prints done)
          lex --list LIST --root ROOT [--version ...] [--doc ...] [--define ...] [--full OUT] [--threads N]
          values --list LIST --root ROOT [--version ...] [--doc ...] [--define ...] [--entry ...] [--kept] [--threads N]
          directives --list LIST --root ROOT [--version ...] [--doc ...] [--define ...] [--entry ...] [--threads N]
          getters --list LIST --root ROOT [--version ...] [--doc ...] [--define ...] [--entry ...] [--full OUT] [--threads N]
          goldens --src ROSLYN_TREE --out DIR [--threads N]
          unicode
        """;

    static int Main(string[] args)
    {
        CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
        CultureInfo.DefaultThreadCurrentUICulture = CultureInfo.InvariantCulture;
        Thread.CurrentThread.CurrentCulture = CultureInfo.InvariantCulture;
        Thread.CurrentThread.CurrentUICulture = CultureInfo.InvariantCulture;
        if (args.Length == 0)
        {
            Console.Error.Write(Usage);
            return 2;
        }
        try
        {
            var a = Args.Parse(args[1..]);
            return args[0] switch
            {
                "dump" => DumpMode.Run(a),
                "lex" => LexMode.Run(a),
                "values" => ValuesMode.Run(a),
                "directives" => DirectivesMode.Run(a),
                "getters" => GettersMode.Run(a),
                "goldens" => Goldens.Run(a),
                "unicode" => UnicodeMode.Run(),
                _ => throw new UsageException($"unknown mode {args[0]}"),
            };
        }
        catch (UsageException e)
        {
            Console.Error.WriteLine(e.Message);
            Console.Error.Write(Usage);
            return 2;
        }
    }
}

sealed class UsageException(string message) : Exception(message);

/// <summary><c>--key value</c> pairs and bare <c>--flag</c>s.</summary>
sealed class Args
{
    readonly Dictionary<string, string?> _values = new();

    public static Args Parse(string[] args)
    {
        var a = new Args();
        for (var i = 0; i < args.Length; i++)
        {
            var key = args[i];
            if (!key.StartsWith("--", StringComparison.Ordinal))
            {
                throw new UsageException($"unexpected argument {key}");
            }
            var name = key[2..];
            if (name is "serve" or "check-interpolation" or "kept")
            {
                a._values[name] = null;
            }
            else if (i + 1 < args.Length)
            {
                a._values[name] = args[++i];
            }
            else
            {
                throw new UsageException($"{key} needs a value");
            }
        }
        return a;
    }

    public bool Has(string name) => _values.ContainsKey(name);

    public string? Get(string name) => _values.TryGetValue(name, out var v) ? v : null;

    public string Need(string name) => Get(name) ?? throw new UsageException($"--{name} is required");

    public int Int(string name, int fallback) =>
        Get(name) is { } v ? int.Parse(v, CultureInfo.InvariantCulture) : fallback;

    public int Threads => Math.Max(1, Int("threads", Environment.ProcessorCount));

    /// <summary>The stack of every worker thread: all files parse on threads of this one size, so the output is
    /// the same at any thread count (Roslyn's stack guard depends on the stack left).</summary>
    public int StackBytes => Int("stack-mb", 64) * 1024 * 1024;
}

static class UnicodeMode
{
    /// <summary><c>CharUnicodeInfo.GetUnicodeCategory</c> of every code point 0..10FFFF as runs
    /// <c>FIRST&lt;TAB&gt;LAST&lt;TAB&gt;Category</c> (hex): what Roslyn's CharacterInfo asks of the runtime.</summary>
    public static int Run()
    {
        var o = new StringBuilder();
        o.Append("# CharUnicodeInfo.GetUnicodeCategory, .NET ").Append(Environment.Version).Append('\n');
        var start = 0;
        var cat = CharUnicodeInfo.GetUnicodeCategory(0);
        for (var cp = 1; cp <= 0x110000; cp++)
        {
            var c = cp <= 0x10FFFF ? CharUnicodeInfo.GetUnicodeCategory(cp) : (UnicodeCategory)(-1);
            if (c != cat)
            {
                o.Append(start.ToString("X4", CultureInfo.InvariantCulture)).Append('\t')
                    .Append((cp - 1).ToString("X4", CultureInfo.InvariantCulture)).Append('\t').Append(cat).Append('\n');
                start = cp;
                cat = c;
            }
        }
        using var stdout = Console.OpenStandardOutput();
        stdout.Write(Encoding.UTF8.GetBytes(o.ToString()));
        return 0;
    }
}
