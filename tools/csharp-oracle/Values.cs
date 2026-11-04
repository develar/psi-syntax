// `values` (README.md, "Value texts"): the tokens whose `ValueText` is not their `Text`, what
// `csharp_syntax::Tree::value_text` must answer for them; with `--kept` only those whose value the port keeps.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace CSharpOracle;

static class ValuesMode
{
    static readonly UTF8Encoding s_utf8 = new(false);

    /// <summary>The kinds whose value the port does not keep (<c>value_not_kept</c> of
    /// crates/csharp-syntax/src/tree.rs): a number's, a character's, a string's content, which no tree shows.
    /// <c>--kept</c> leaves them out, a string of a directive aside.</summary>
    static readonly HashSet<SyntaxKind> s_notKept =
    [
        SyntaxKind.NumericLiteralToken,
        SyntaxKind.CharacterLiteralToken,
        SyntaxKind.StringLiteralToken,
        SyntaxKind.Utf8StringLiteralToken,
        SyntaxKind.SingleLineRawStringLiteralToken,
        SyntaxKind.MultiLineRawStringLiteralToken,
        SyntaxKind.Utf8SingleLineRawStringLiteralToken,
        SyntaxKind.Utf8MultiLineRawStringLiteralToken,
        SyntaxKind.InterpolatedStringToken,
        SyntaxKind.InterpolatedStringTextToken,
    ];

    /// <summary>Whether the port keeps <paramref name="token"/>'s value: every kind but <c>s_notKept</c>'s, and a
    /// string of a directive (<c>#line 1 "a.cs"</c>, <c>#pragma checksum</c>, <c>#r</c>, <c>#load</c>, a skipped
    /// string of a directive line), which the directive lexer gives its value.</summary>
    static bool Kept(SyntaxToken token) =>
        !s_notKept.Contains(token.Kind())
        || (token.IsKind(SyntaxKind.StringLiteralToken)
            && token.Parent?.FirstAncestorOrSelf<DirectiveTriviaSyntax>(ascendOutOfTrivia: true) != null);

    public static int Run(Args a)
    {
        var defaults = Options.FromArgs(a);
        var jobs = Job.ReadList(a.Need("list"), Options.ParseEntry(a.Get("entry") ?? ""), defaults);
        var root = a.Need("root");
        var kept = a.Has("kept");
        var outputs = new string[jobs.Count];
        Workers.Run(jobs.Count, a.Threads, a.StackBytes, i =>
        {
            var job = jobs[i];
            var o = new StringBuilder();
            try
            {
                var text = Dumper.Decode(File.ReadAllBytes(Path.Combine(root, job.Path)));
                if (text == null)
                {
                    o.Append(job.Path).Append("\tfailure\tinvalid UTF-8\n");
                }
                else
                {
                    Lines(o, job.Path, text, job.Entry, job.Options, kept);
                }
            }
            catch (Exception e) when (e is not OutOfMemoryException)
            {
                o.Clear().Append(job.Path).Append("\tfailure\t").Append(e.GetType().Name).Append(": ")
                    .Append(e.Message.Replace('\n', ' ').Replace('\t', ' ')).Append('\n');
            }
            outputs[i] = o.ToString();
        });
        using var stdout = Console.OpenStandardOutput();
        foreach (var output in outputs)
        {
            stdout.Write(s_utf8.GetBytes(output));
        }
        return 0;
    }

    /// <summary>One line per token of the parse whose <c>ValueText</c> is not its <c>Text</c>, in document order,
    /// the tokens of structured trivia (directives, documentation comments, skipped tokens) included:
    /// <c>PATH&lt;TAB&gt;START&lt;TAB&gt;Kind&lt;TAB&gt;'value'</c>, START the token's span start in UTF-8 bytes,
    /// the value with the dump's text escapes; with <paramref name="kept"/> only the tokens whose value the port keeps
    /// (<c>Kept</c>).</summary>
    static void Lines(StringBuilder o, string path, string text, Entry entry, Options options, bool kept)
    {
        var utf8 = Printer.Utf8Offsets(text);
        var po = options.Parse;
        IEnumerable<SyntaxToken> tokens;
        if (entry == Entry.Tokens)
        {
            tokens = SyntaxFactory.ParseTokens(text, options: po).SelectMany(WithStructure);
        }
        else
        {
            SyntaxNode? node = entry switch
            {
                Entry.CompilationUnit => CSharpSyntaxTree.ParseText(text, po).GetRoot(),
                Entry.Expression => SyntaxFactory.ParseExpression(text, 0, po, consumeFullText: true),
                Entry.Statement => SyntaxFactory.ParseStatement(text, 0, po, consumeFullText: true),
                Entry.Member => SyntaxFactory.ParseMemberDeclaration(text, 0, po, consumeFullText: true),
                Entry.TypeName => SyntaxFactory.ParseTypeName(text, 0, po, consumeFullText: true),
                Entry.Name => SyntaxFactory.ParseName(text, 0, consumeFullText: true),
                Entry.ParameterList => SyntaxFactory.ParseParameterList(text, 0, po, consumeFullText: true),
                Entry.BracketedParameterList => SyntaxFactory.ParseBracketedParameterList(text, 0, po, consumeFullText: true),
                Entry.ArgumentList => SyntaxFactory.ParseArgumentList(text, 0, po, consumeFullText: true),
                Entry.BracketedArgumentList => SyntaxFactory.ParseBracketedArgumentList(text, 0, po, consumeFullText: true),
                Entry.AttributeArgumentList => SyntaxFactory.ParseAttributeArgumentList(text, 0, po, consumeFullText: true),
                _ => throw new ArgumentOutOfRangeException(nameof(entry)),
            };
            if (node == null)
            {
                return;
            }
            tokens = node.DescendantTokens(descendIntoTrivia: true);
        }
        foreach (var token in tokens)
        {
            if (token.ValueText == token.Text || (kept && !Kept(token)))
            {
                continue;
            }
            var start = token.SpanStart;
            o.Append(path).Append('\t').Append(start < utf8.Length ? utf8[start] : utf8[^1]).Append('\t')
                .Append(token.Kind()).Append("\t'");
            Printer.Escape(o, token.ValueText);
            o.Append("'\n");
        }
    }

    /// <summary>A lexed token, then the tokens of its structured trivia.</summary>
    static IEnumerable<SyntaxToken> WithStructure(SyntaxToken token)
    {
        foreach (var t in token.LeadingTrivia.Where(t => t.HasStructure))
        {
            foreach (var inner in t.GetStructure()!.DescendantTokens(descendIntoTrivia: true))
            {
                yield return inner;
            }
        }
        yield return token;
        foreach (var t in token.TrailingTrivia.Where(t => t.HasStructure))
        {
            foreach (var inner in t.GetStructure()!.DescendantTokens(descendIntoTrivia: true))
            {
                yield return inner;
            }
        }
    }
}
