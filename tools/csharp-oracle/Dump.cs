// The dump (README.md, "The dump"): one line per node, token and trivia, UTF-8 byte offsets, Roslyn's diagnostics.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;
using Microsoft.CodeAnalysis.Text;

namespace CSharpOracle;

/// <summary>What the parser is asked for: the root a test or a file is parsed to.</summary>
enum Entry
{
    CompilationUnit,
    Expression,
    Statement,
    Member,
    TypeName,
    Name,
    ParameterList,
    BracketedParameterList,
    ArgumentList,
    BracketedArgumentList,
    AttributeArgumentList,
    Tokens,
}

/// <summary>A file's parse options: the language version as asked (`latest`, `preview` or a number), the
/// documentation mode and the preprocessor symbols, their names joined by <c>;</c> (empty: none). Script and features
/// are out of scope.</summary>
readonly record struct Options(LanguageVersion Version, DocumentationMode Doc, string Symbols = "")
{
    public static readonly Options Default = new(LanguageVersion.Latest, DocumentationMode.Parse);

    static readonly ConcurrentDictionary<Options, CSharpParseOptions> s_parse = new();

    public CSharpParseOptions Parse => s_parse.GetOrAdd(this, o =>
    {
        var po = new CSharpParseOptions(languageVersion: o.Version, documentationMode: o.Doc, kind: SourceCodeKind.Regular);
        return o.Symbols == "" ? po : po.WithPreprocessorSymbols(o.Symbols.Split(';'));
    });

    /// <summary>The defaults of a command: <c>--version</c>, <c>--doc</c> and <c>--define A;B</c>.</summary>
    public static Options FromArgs(Args a) =>
        new(ParseVersion(a.Get("version") ?? ""), ParseDoc(a.Get("doc") ?? ""), ParseSymbols(a.Get("define") ?? ""));

    /// <summary>A SYMBOLS column or <c>--define</c>: names separated by <c>;</c>, empty names dropped.</summary>
    public static string ParseSymbols(string s) => string.Join(';', s.Split(';', StringSplitOptions.RemoveEmptyEntries));

    public static LanguageVersion ParseVersion(string s)
    {
        if (s == "")
        {
            return Default.Version;
        }
        if (!LanguageVersionFacts.TryParse(s, out var v))
        {
            throw new UsageException($"unknown language version {s}");
        }
        return v;
    }

    /// <summary>The manifest's spelling of a version: <c>latest</c> for every version that maps to the latest
    /// release (Default, Latest, LatestMajor, the newest number), <c>preview</c>, else the number without a
    /// trailing <c>.0</c> (<c>5</c>, <c>7.3</c>, <c>13</c>).</summary>
    public static string VersionName(LanguageVersion v)
    {
        var effective = v.MapSpecifiedToEffectiveVersion();
        if (effective == LanguageVersion.Preview)
        {
            return "preview";
        }
        if (effective == LanguageVersion.Latest.MapSpecifiedToEffectiveVersion())
        {
            return "latest";
        }
        var s = effective.ToDisplayString();
        return s.EndsWith(".0", StringComparison.Ordinal) ? s[..^2] : s;
    }

    public static DocumentationMode ParseDoc(string s) => s switch
    {
        "" or "parse" => DocumentationMode.Parse,
        "none" => DocumentationMode.None,
        "diagnose" => DocumentationMode.Diagnose,
        _ => throw new UsageException($"unknown documentation mode {s}"),
    };

    public static string DocName(DocumentationMode d) => d switch
    {
        DocumentationMode.Parse => "parse",
        DocumentationMode.None => "none",
        DocumentationMode.Diagnose => "diagnose",
        _ => throw new ArgumentOutOfRangeException(nameof(d)),
    };

    public static Entry ParseEntry(string s) => s switch
    {
        "" or "compilation-unit" => Entry.CompilationUnit,
        "expression" => Entry.Expression,
        "statement" => Entry.Statement,
        "member" => Entry.Member,
        "type-name" => Entry.TypeName,
        "name" => Entry.Name,
        "parameter-list" => Entry.ParameterList,
        "bracketed-parameter-list" => Entry.BracketedParameterList,
        "argument-list" => Entry.ArgumentList,
        "bracketed-argument-list" => Entry.BracketedArgumentList,
        "attribute-argument-list" => Entry.AttributeArgumentList,
        "tokens" => Entry.Tokens,
        _ => throw new UsageException($"unknown entry {s}"),
    };

    public static string EntryName(Entry e) => e switch
    {
        Entry.CompilationUnit => "compilation-unit",
        Entry.Expression => "expression",
        Entry.Statement => "statement",
        Entry.Member => "member",
        Entry.TypeName => "type-name",
        Entry.Name => "name",
        Entry.ParameterList => "parameter-list",
        Entry.BracketedParameterList => "bracketed-parameter-list",
        Entry.ArgumentList => "argument-list",
        Entry.BracketedArgumentList => "bracketed-argument-list",
        Entry.AttributeArgumentList => "attribute-argument-list",
        Entry.Tokens => "tokens",
        _ => throw new ArgumentOutOfRangeException(nameof(e)),
    };
}

/// <summary>Prints one parse result. Offsets are UTF-8 byte offsets of the text it was made with.</summary>
sealed class Printer
{
    static readonly string?[] s_kindNames = new string?[ushort.MaxValue + 1];

    readonly StringBuilder _out = new();
    readonly int[] _utf8;

    public Printer(string text)
    {
        _utf8 = Utf8Offsets(text);
    }

    /// <summary>The UTF-8 byte offset of every UTF-16 position 0..Length. A position between the two halves of a
    /// supplementary character maps to that character's offset + 2 (each half counts 2 of its 4 bytes); a lone
    /// surrogate (never in decoded UTF-8) would count 3, as U+FFFD.</summary>
    public static int[] Utf8Offsets(string text)
    {
        var map = new int[text.Length + 1];
        var b = 0;
        for (var i = 0; i < text.Length; i++)
        {
            map[i] = b;
            var c = text[i];
            if (c < 0x80)
            {
                b += 1;
            }
            else if (c < 0x800)
            {
                b += 2;
            }
            else if (char.IsHighSurrogate(c) && i + 1 < text.Length && char.IsLowSurrogate(text[i + 1]))
            {
                b += 2;
            }
            else if (char.IsLowSurrogate(c) && i > 0 && char.IsHighSurrogate(text[i - 1]))
            {
                b += 2;
            }
            else
            {
                b += 3;
            }
        }
        map[text.Length] = b;
        return map;
    }

    public override string ToString() => _out.ToString();

    static string KindName(SyntaxKind kind)
    {
        var i = (ushort)kind;
        return s_kindNames[i] ??= kind.ToString();
    }

    void Span(TextSpan span)
    {
        _out.Append(" [").Append(_utf8[span.Start]).Append("..").Append(_utf8[span.End]).Append(')');
    }

    void Indent(int depth) => _out.Append(' ', depth * 2);

    /// <summary>The text escapes of README.md: <c>\\ \' \n \r \t</c>, <c>\u{XXXX}</c> for the other controls below
    /// U+0020, U+007F, U+0085, U+2028, U+2029, U+FEFF and a lone surrogate; anything else as itself.</summary>
    public static void Escape(StringBuilder o, string s)
    {
        for (var i = 0; i < s.Length; i++)
        {
            var c = s[i];
            switch (c)
            {
                case '\\': o.Append("\\\\"); continue;
                case '\'': o.Append("\\'"); continue;
                case '\n': o.Append("\\n"); continue;
                case '\r': o.Append("\\r"); continue;
                case '\t': o.Append("\\t"); continue;
            }
            if (char.IsHighSurrogate(c) && i + 1 < s.Length && char.IsLowSurrogate(s[i + 1]))
            {
                o.Append(c).Append(s[++i]);
            }
            else if (c < 0x20 || c == 0x7F || c == 0x85 || c == 0x2028 || c == 0x2029 || c == 0xFEFF ||
                     char.IsSurrogate(c))
            {
                o.Append("\\u{").Append(((int)c).ToString("X4", CultureInfo.InvariantCulture)).Append('}');
            }
            else
            {
                o.Append(c);
            }
        }
    }

    void Quoted(string s)
    {
        _out.Append(" '");
        Escape(_out, s);
        _out.Append('\'');
    }

    enum Work : byte { Node, Token, TokenSelf, Trivia }

    readonly record struct Item(Work Work, int Depth, SyntaxNode? Node, SyntaxToken Token, SyntaxTrivia Trivia, bool Trailing);

    readonly Stack<Item> _stack = new();
    readonly List<SyntaxNodeOrToken> _children = new();

    void PushChildren(SyntaxNode node, int depth)
    {
        _children.Clear();
        foreach (var child in node.ChildNodesAndTokens())
        {
            _children.Add(child);
        }
        for (var i = _children.Count - 1; i >= 0; i--)
        {
            var c = _children[i];
            _stack.Push(c.IsNode
                ? new Item(Work.Node, depth, c.AsNode(), default, default, false)
                : new Item(Work.Token, depth, null, c.AsToken(), default, false));
        }
    }

    void PushToken(SyntaxToken token, int depth)
    {
        var trailing = token.TrailingTrivia;
        for (var i = trailing.Count - 1; i >= 0; i--)
        {
            _stack.Push(new Item(Work.Trivia, depth, null, default, trailing[i], true));
        }
        _stack.Push(new Item(Work.TokenSelf, depth, null, token, default, false));
        var leading = token.LeadingTrivia;
        for (var i = leading.Count - 1; i >= 0; i--)
        {
            _stack.Push(new Item(Work.Trivia, depth, null, default, leading[i], false));
        }
    }

    /// <summary>The longest dump printed, in chars: past it a file is a failure (<c>dump over 512 MB</c>), as
    /// the 24 MB nested expression of dotnet/runtime's hugeexpr1.cs, whose lines are mostly indentation.</summary>
    public const int MaxChars = 1 << 29;

    void Drain()
    {
        while (_stack.Count > 0)
        {
            if (_out.Length > MaxChars)
            {
                throw new DumpTooLargeException();
            }
            var item = _stack.Pop();
            switch (item.Work)
            {
                case Work.Node:
                    Indent(item.Depth);
                    _out.Append(KindName(item.Node!.Kind()));
                    Span(item.Node!.FullSpan);
                    _out.Append('\n');
                    PushChildren(item.Node, item.Depth + 1);
                    break;
                case Work.Token:
                    PushToken(item.Token, item.Depth);
                    break;
                case Work.TokenSelf:
                    Indent(item.Depth);
                    _out.Append(KindName(item.Token.Kind()));
                    Span(item.Token.Span);
                    if (item.Token.IsMissing)
                    {
                        _out.Append(" missing");
                    }
                    else
                    {
                        Quoted(item.Token.Text);
                    }
                    _out.Append('\n');
                    break;
                case Work.Trivia:
                    Indent(item.Depth);
                    _out.Append(item.Trailing ? "T:" : "L:").Append(KindName(item.Trivia.Kind()));
                    Span(item.Trivia.FullSpan);
                    if (item.Trivia.HasStructure)
                    {
                        _out.Append('\n');
                        PushChildren(item.Trivia.GetStructure()!, item.Depth + 1);
                    }
                    else
                    {
                        Quoted(item.Trivia.ToFullString());
                        _out.Append('\n');
                    }
                    break;
            }
        }
    }

    public void Node(SyntaxNode node)
    {
        _stack.Push(new Item(Work.Node, 0, node, default, default, false));
        Drain();
    }

    public void Token(SyntaxToken token)
    {
        PushToken(token, 0);
        Drain();
    }

    public void Trivia(SyntaxTrivia trivia)
    {
        _stack.Push(new Item(Work.Trivia, 0, null, default, trivia, false));
        Drain();
    }

    /// <summary>Appends the <c>Diagnostics:</c> section; returns the count and the first line (without its
    /// indentation).</summary>
    public (int Count, string? First) Diagnostics(IEnumerable<Diag> diagnostics)
    {
        var count = 0;
        string? first = null;
        foreach (var d in diagnostics)
        {
            if (count == 0)
            {
                _out.Append("Diagnostics:\n");
            }
            var start = _out.Length + 2;
            _out.Append("  ").Append(d.Id);
            Span(d.Span);
            _out.Append(' ');
            Escape(_out, d.Message);
            first ??= _out.ToString(start, _out.Length - start);
            _out.Append('\n');
            count++;
        }
        return (count, first);
    }
}

/// <summary>A diagnostic as the dump prints it: its id, its span in UTF-16 positions, its invariant message.</summary>
readonly record struct Diag(string Id, TextSpan Span, string Message)
{
    /// <summary>The directory of the Roslyn assembly the oracle runs, with its separator.</summary>
    static readonly string s_compilerDirectory =
        Path.GetDirectoryName(typeof(CSharpSyntaxTree).Assembly.Location) + Path.DirectorySeparatorChar;

    public static IEnumerable<Diag> Of(IEnumerable<Diagnostic> diagnostics) => diagnostics.Select(d =>
        new Diag(d.Id, d.Location.IsInSource ? d.Location.SourceSpan : default, MessageOf(d)));

    /// <summary><c>GetMessage(CultureInfo.InvariantCulture)</c>, without the directory of the compiler: CS8304
    /// (<c>#error version</c>) names the running Roslyn assembly's path, which is the build's, so the oracle prints
    /// its file name alone (<c>Compiler path: 'Microsoft.CodeAnalysis.CSharp.dll'</c>).</summary>
    public static string MessageOf(Diagnostic d) =>
        d.GetMessage(CultureInfo.InvariantCulture).Replace(s_compilerDirectory, "", StringComparison.Ordinal);
}

sealed class DumpTooLargeException() : Exception("dump over 512 MB");

/// <summary>A dump of one text: the file's bytes, or why there is none.</summary>
readonly record struct DumpResult(string? Text, int Errors, string? FirstError, int Tokens, string? Failure);

static class Dumper
{
    static readonly UTF8Encoding s_strict = new(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);

    public static string? Decode(byte[] bytes)
    {
        try
        {
            return s_strict.GetString(bytes);
        }
        catch (DecoderFallbackException)
        {
            return null;
        }
    }

    public static DumpResult Dump(string text, Entry entry, Options options)
    {
        var p = new Printer(text);
        var po = options.Parse;
        IEnumerable<Diag> diagnostics;
        var tokens = 0;
        switch (entry)
        {
            case Entry.CompilationUnit:
            {
                var tree = CSharpSyntaxTree.ParseText(text, po);
                p.Node(tree.GetRoot());
                diagnostics = Diag.Of(tree.GetDiagnostics());
                break;
            }
            case Entry.Tokens:
            {
                var list = SyntaxFactory.ParseTokens(text, options: po).ToList();
                foreach (var t in list)
                {
                    p.Token(t);
                }
                tokens = list.Count;
                diagnostics = TokenDiagnostics(list, po);
                break;
            }
            default:
            {
                SyntaxNode? node = entry switch
                {
                    Entry.Expression => SyntaxFactory.ParseExpression(text, 0, po, consumeFullText: true),
                    Entry.Statement => SyntaxFactory.ParseStatement(text, 0, po, consumeFullText: true),
                    Entry.Member => SyntaxFactory.ParseMemberDeclaration(text, 0, po, consumeFullText: true),
                    Entry.TypeName => SyntaxFactory.ParseTypeName(text, 0, po, consumeFullText: true),
                    // Roslyn's ParseName takes no options: it parses with CSharpParseOptions.Default.
                    Entry.Name => SyntaxFactory.ParseName(text, 0, consumeFullText: true),
                    Entry.ParameterList => SyntaxFactory.ParseParameterList(text, 0, po, consumeFullText: true),
                    Entry.BracketedParameterList =>
                        SyntaxFactory.ParseBracketedParameterList(text, 0, po, consumeFullText: true),
                    Entry.ArgumentList => SyntaxFactory.ParseArgumentList(text, 0, po, consumeFullText: true),
                    Entry.BracketedArgumentList =>
                        SyntaxFactory.ParseBracketedArgumentList(text, 0, po, consumeFullText: true),
                    Entry.AttributeArgumentList =>
                        SyntaxFactory.ParseAttributeArgumentList(text, 0, po, consumeFullText: true),
                    _ => throw new ArgumentOutOfRangeException(nameof(entry)),
                };
                if (node == null)
                {
                    return new DumpResult(null, 0, null, 0, "no " + Options.EntryName(entry));
                }
                p.Node(node);
                diagnostics = Diag.Of(node.GetDiagnostics());
                break;
            }
        }
        var (errors, first) = p.Diagnostics(diagnostics);
        return new DumpResult(p.ToString(), errors, first, tokens, null);
    }

    /// <summary>The diagnostics of lexed tokens in the order a parse reports them. Tokens out of
    /// <c>ParseTokens</c> belong to no tree, so their own <c>GetDiagnostics()</c> has no location: each token with
    /// a diagnostic is put back into one, as the skipped-tokens trivia of an empty compilation unit's end-of-file
    /// token (the end-of-file token itself as that token), whose <c>GetDiagnostics()</c> walks the token and its
    /// trivia in document order with spans from 0, moved to the token's full start. One tree for all the tokens
    /// gives the same diagnostics in the same order, but in time quadratic in their count.</summary>
    static IEnumerable<Diag> TokenDiagnostics(List<SyntaxToken> tokens, CSharpParseOptions options)
    {
        var result = new List<Diag>();
        for (var i = 0; i < tokens.Count; i++)
        {
            var token = tokens[i];
            if (!token.ContainsDiagnostics)
            {
                continue;
            }
            var root = i == tokens.Count - 1
                ? SyntaxFactory.CompilationUnit().WithEndOfFileToken(token)
                : SyntaxFactory.CompilationUnit().WithEndOfFileToken(SyntaxFactory.Token(
                    SyntaxFactory.TriviaList(SyntaxFactory.Trivia(SyntaxFactory.SkippedTokensTrivia(SyntaxFactory.TokenList(token)))),
                    SyntaxKind.EndOfFileToken, default));
            if (root.FullSpan.Length != token.FullSpan.Length)
            {
                throw new InvalidOperationException("a lexed token's tree does not spell it");
            }
            var shift = token.FullSpan.Start;
            foreach (var d in CSharpSyntaxTree.Create(root, options).GetDiagnostics())
            {
                var span = d.Location.IsInSource ? d.Location.SourceSpan : default;
                result.Add(new Diag(d.Id, new TextSpan(span.Start + shift, span.Length), Diag.MessageOf(d)));
            }
        }
        return result;
    }

    /// <summary>FNV-1a 64 of the bytes, 16 lowercase hex digits.</summary>
    public static string Fnv(byte[] bytes)
    {
        var h = 0xcbf29ce484222325UL;
        foreach (var b in bytes)
        {
            h ^= b;
            h *= 0x100000001b3UL;
        }
        return h.ToString("x16", CultureInfo.InvariantCulture);
    }
}

/// <summary>A list line: <c>PATH[&lt;TAB&gt;VERSION[&lt;TAB&gt;ENTRY[&lt;TAB&gt;DOC[&lt;TAB&gt;SYMBOLS]]]]</c>, an empty
/// or absent column taking the command's default; more columns are ignored.</summary>
readonly record struct Job(string Path, Entry Entry, Options Options)
{
    public static List<Job> ReadList(string list, Entry entry, Options defaults)
    {
        var jobs = new List<Job>();
        foreach (var raw in File.ReadAllLines(list, Encoding.UTF8))
        {
            if (raw.Length == 0)
            {
                continue;
            }
            var cols = raw.Split('\t');
            var version = cols.Length > 1 && cols[1] != "" ? Options.ParseVersion(cols[1]) : defaults.Version;
            var e = cols.Length > 2 && cols[2] != "" ? Options.ParseEntry(cols[2]) : entry;
            var doc = cols.Length > 3 && cols[3] != "" ? Options.ParseDoc(cols[3]) : defaults.Doc;
            var symbols = cols.Length > 4 && cols[4] != "" ? Options.ParseSymbols(cols[4]) : defaults.Symbols;
            jobs.Add(new Job(cols[0], e, new Options(version, doc, symbols)));
        }
        return jobs;
    }
}

static class Workers
{
    /// <summary>Runs <paramref name="work"/> for 0..count on <paramref name="threads"/> threads of one stack size,
    /// each index once.</summary>
    public static void Run(int count, int threads, int stackBytes, Action<int> work)
    {
        var next = -1;
        Exception? failure = null;
        var pool = new List<Thread>();
        for (var t = 0; t < Math.Min(threads, Math.Max(count, 1)); t++)
        {
            var thread = new Thread(() =>
            {
                Thread.CurrentThread.CurrentCulture = CultureInfo.InvariantCulture;
                Thread.CurrentThread.CurrentUICulture = CultureInfo.InvariantCulture;
                try
                {
                    int i;
                    while ((i = Interlocked.Increment(ref next)) < count)
                    {
                        work(i);
                    }
                }
                catch (Exception e)
                {
                    Interlocked.CompareExchange(ref failure, e, null);
                }
            }, stackBytes);
            thread.Start();
            pool.Add(thread);
        }
        foreach (var thread in pool)
        {
            thread.Join();
        }
        if (failure != null)
        {
            throw new AggregateException(failure);
        }
    }
}

static class Files
{
    /// <summary>Writes a new file and renames it over <paramref name="path"/>: a hard link there (link_same.py) is
    /// replaced, never written through.</summary>
    public static void Replace(string path, byte[] bytes)
    {
        var tmp = path + ".tmp" + Environment.CurrentManagedThreadId;
        File.WriteAllBytes(tmp, bytes);
        File.Move(tmp, path, overwrite: true);
    }
}

static class Json
{
    public static void String(StringBuilder o, string s)
    {
        o.Append('"');
        foreach (var c in s)
        {
            switch (c)
            {
                case '"': o.Append("\\\""); break;
                case '\\': o.Append("\\\\"); break;
                case '\n': o.Append("\\n"); break;
                case '\r': o.Append("\\r"); break;
                case '\t': o.Append("\\t"); break;
                default:
                    if (c < 0x20)
                    {
                        o.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                    }
                    else
                    {
                        o.Append(c);
                    }
                    break;
            }
        }
        o.Append('"');
    }
}

static class DumpMode
{
    static readonly UTF8Encoding s_utf8 = new(false);

    public static int Run(Args a)
    {
        var defaults = Options.FromArgs(a);
        var entry = Options.ParseEntry(a.Get("entry") ?? "");
        if (a.Has("serve"))
        {
            string? line;
            while ((line = Console.In.ReadLine()) != null)
            {
                if (line.Length == 0)
                {
                    continue;
                }
                var cols = line.Split('\t');
                if (cols.Length < 3)
                {
                    Console.Out.WriteLine("error: expected LIST<TAB>ROOT<TAB>OUT");
                    Console.Out.Flush();
                    continue;
                }
                RunList(cols[0], cols[1], cols[2], entry, defaults, a.Threads, a.StackBytes);
                Console.Out.WriteLine("done");
                Console.Out.Flush();
            }
            return 0;
        }
        RunList(a.Need("list"), a.Need("root"), a.Need("out"), entry, defaults, a.Threads, a.StackBytes);
        return 0;
    }

    public static void RunList(string list, string root, string outDir, Entry entry, Options defaults, int threads,
        int stackBytes)
    {
        var jobs = Job.ReadList(list, entry, defaults);
        var index = new string[jobs.Count];
        Directory.CreateDirectory(outDir);
        var started = DateTime.UtcNow;
        Workers.Run(jobs.Count, threads, stackBytes, i =>
        {
            var job = jobs[i];
            var o = new StringBuilder();
            o.Append("{\"path\":");
            Json.String(o, job.Path);
            DumpResult r;
            try
            {
                var bytes = File.ReadAllBytes(Path.Combine(root, job.Path));
                var text = Dumper.Decode(bytes);
                r = text == null ? new DumpResult(null, 0, null, 0, "invalid UTF-8") : Dumper.Dump(text, job.Entry, job.Options);
            }
            catch (DumpTooLargeException e)
            {
                r = new DumpResult(null, 0, null, 0, e.Message);
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                r = new DumpResult(null, 0, null, 0, e.GetType().Name + ": " + e.Message);
            }
            if (r.Text != null)
            {
                var target = Path.Combine(outDir, job.Path + ".tree.txt");
                Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                Files.Replace(target, s_utf8.GetBytes(r.Text));
                o.Append(",\"errors\":").Append(r.Errors).Append(",\"firstError\":");
                if (r.FirstError == null)
                {
                    o.Append("null");
                }
                else
                {
                    Json.String(o, r.FirstError);
                }
            }
            else
            {
                o.Append(",\"failure\":");
                Json.String(o, r.Failure!);
            }
            o.Append('}');
            index[i] = o.ToString();
        });
        Files.Replace(Path.Combine(outDir, "index.jsonl"), s_utf8.GetBytes(string.Concat(index.Select(l => l + "\n"))));
        Console.Error.WriteLine($"dumped {jobs.Count} files in {(DateTime.UtcNow - started).TotalSeconds:F1} s");
    }
}

static class LexMode
{
    static readonly UTF8Encoding s_utf8 = new(false);

    public static int Run(Args a)
    {
        var defaults = Options.FromArgs(a);
        var jobs = Job.ReadList(a.Need("list"), Entry.Tokens, defaults);
        var root = a.Need("root");
        var full = a.Get("full");
        var lines = new string[jobs.Count];
        Workers.Run(jobs.Count, a.Threads, a.StackBytes, i =>
        {
            var job = jobs[i];
            string line;
            try
            {
                var text = Dumper.Decode(File.ReadAllBytes(Path.Combine(root, job.Path)));
                if (text == null)
                {
                    line = job.Path + "\tfailure\tinvalid UTF-8";
                }
                else
                {
                    var r = Dumper.Dump(text, Entry.Tokens, job.Options);
                    var bytes = s_utf8.GetBytes(r.Text!);
                    line = job.Path + "\t" + r.Tokens + "\t" + Dumper.Fnv(bytes);
                    if (full != null)
                    {
                        var target = Path.Combine(full, job.Path + ".lex.txt");
                        Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                        Files.Replace(target, bytes);
                    }
                }
            }
            catch (DumpTooLargeException e)
            {
                line = job.Path + "\tfailure\t" + e.Message;
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                line = job.Path + "\tfailure\t" + e.GetType().Name + ": " + e.Message.Replace('\n', ' ').Replace('\t', ' ');
            }
            lines[i] = line;
        });
        using var stdout = Console.OpenStandardOutput();
        var o = new StringBuilder();
        foreach (var l in lines)
        {
            o.Append(l).Append('\n');
        }
        stdout.Write(s_utf8.GetBytes(o.ToString()));
        return 0;
    }
}
