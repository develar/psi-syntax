// `directives` (README.md, "Directives"): each directive's boolean fields, what `csharp_syntax::Tree::directives`
// must answer for it.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace CSharpOracle;

static class DirectivesMode
{
    static readonly UTF8Encoding s_utf8 = new(false);

    public static int Run(Args a)
    {
        var defaults = Options.FromArgs(a);
        var jobs = Job.ReadList(a.Need("list"), Options.ParseEntry(a.Get("entry") ?? ""), defaults);
        var root = a.Need("root");
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
                    Lines(o, job.Path, text, job.Entry, job.Options);
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

    /// <summary>One line per directive of the parse (a trivia whose <c>IsDirective</c>), in document order, those in
    /// other structured trivia included: <c>PATH&lt;TAB&gt;Kind&lt;TAB&gt;START&lt;TAB&gt;END&lt;TAB&gt;FLAGS</c>,
    /// START and END the trivia's <c>FullSpan</c> in UTF-8 bytes, FLAGS three characters: <c>A</c> when
    /// <c>IsActive</c>, <c>T</c> when <c>BranchTaken</c> (a <c>BranchingDirectiveTriviaSyntax</c>: <c>#if</c>,
    /// <c>#elif</c>, <c>#else</c>), <c>V</c> when <c>ConditionValue</c> (a <c>ConditionalDirectiveTriviaSyntax</c>:
    /// <c>#if</c>, <c>#elif</c>), each else <c>-</c>.</summary>
    static void Lines(StringBuilder o, string path, string text, Entry entry, Options options)
    {
        var utf8 = Printer.Utf8Offsets(text);
        IEnumerable<SyntaxTrivia> trivia;
        if (entry == Entry.Tokens)
        {
            trivia = SyntaxFactory.ParseTokens(text, options: options.Parse)
                .SelectMany(t => t.LeadingTrivia.Concat(t.TrailingTrivia))
                .SelectMany(WithStructure);
        }
        else
        {
            var node = Dumper.Root(text, entry, options.Parse);
            if (node == null)
            {
                return;
            }
            trivia = node.DescendantTrivia(descendIntoTrivia: true);
        }
        foreach (var t in trivia)
        {
            if (!t.IsDirective || t.GetStructure() is not DirectiveTriviaSyntax directive)
            {
                continue;
            }
            var span = t.FullSpan;
            o.Append(path).Append('\t').Append(t.Kind()).Append('\t').Append(utf8[span.Start]).Append('\t')
                .Append(utf8[span.End]).Append('\t')
                .Append(directive.IsActive ? 'A' : '-')
                .Append(directive is BranchingDirectiveTriviaSyntax { BranchTaken: true } ? 'T' : '-')
                .Append(directive is ConditionalDirectiveTriviaSyntax { ConditionValue: true } ? 'V' : '-')
                .Append('\n');
        }
    }

    /// <summary>A lexed token's trivia, then the trivia of its structure, as <c>DescendantTrivia</c> walks a
    /// tree.</summary>
    static IEnumerable<SyntaxTrivia> WithStructure(SyntaxTrivia trivia)
    {
        yield return trivia;
        if (trivia.HasStructure)
        {
            foreach (var inner in trivia.GetStructure()!.DescendantTrivia(descendIntoTrivia: true))
            {
                yield return inner;
            }
        }
    }
}
