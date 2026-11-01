// The lexer oracle: lexes each listed file with Kotlin 2.4.21's KotlinLexer (and every doc comment's text with
// KDocLexer) and prints one line per file: "<path>\t<fnv64 hex>\t<tokens>", hashing "<debug name>:<utf-8 length>;" per
// token, KDoc tokens of a doc comment following it between "[" and "]". `ksdump --lex` prints the same lines for our
// lexers, so `diff` finds every file where they disagree; --dump prints the token lists themselves.
//
//   java -cp "$HOME/.cache/kotlin-syntax/psi-tools/tools/psi-oracle/lib/*" scripts/LexOracle.java \
//     --list LIST --root ROOT [--dump]
//
// The text is read as UTF-8 with CRLF and CR converted to LF, as the PSI oracle does.

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

import org.jetbrains.kotlin.com.intellij.lexer.Lexer;
import org.jetbrains.kotlin.com.intellij.psi.tree.IElementType;
import org.jetbrains.kotlin.kdoc.lexer.KDocLexer;
import org.jetbrains.kotlin.lexer.KotlinLexer;

public class LexOracle {
  static long hash;
  static int count;
  static StringBuilder dump;

  public static void main(String[] args) throws Exception {
    Path list = null;
    Path root = Path.of(".");
    boolean dumpTokens = false;
    for (int i = 0; i < args.length; i++) {
      switch (args[i]) {
        case "--list" -> list = Path.of(args[++i]);
        case "--root" -> root = Path.of(args[++i]);
        case "--dump" -> dumpTokens = true;
        default -> throw new IllegalArgumentException(args[i]);
      }
    }
    List<String> paths = Files.readAllLines(list, StandardCharsets.UTF_8);
    StringBuilder out = new StringBuilder();
    for (String path : paths) {
      if (path.isBlank()) continue;
      String text;
      try {
        text = Files.readString(root.resolve(path), StandardCharsets.UTF_8);
      }
      catch (Exception e) {
        out.append(path).append("\tunreadable\t0\n");
        continue;
      }
      text = text.replace("\r\n", "\n").replace('\r', '\n');
      hash = 0xcbf29ce484222325L;
      count = 0;
      dump = dumpTokens ? new StringBuilder() : null;
      lex(new KotlinLexer(), text, true);
      out.append(path).append('\t').append(Long.toHexString(hash)).append('\t').append(count).append('\n');
      if (dump != null) out.append(dump);
      if (out.length() > 1 << 20) {
        System.out.print(out);
        out.setLength(0);
      }
    }
    System.out.print(out);
    System.out.flush();
  }

  static void lex(Lexer lexer, String text, boolean kotlin) {
    lexer.start(text);
    while (true) {
      IElementType type = lexer.getTokenType();
      if (type == null) break;
      String tokenText = text.substring(lexer.getTokenStart(), lexer.getTokenEnd());
      feed(type + ":" + tokenText.getBytes(StandardCharsets.UTF_8).length + ";");
      if (dump != null) dump.append(kotlin ? "  " : "    ").append(type).append(' ').append(escape(tokenText)).append('\n');
      if (kotlin && type.toString().equals("KDoc")) {
        feed("[");
        lex(new KDocLexer(), tokenText, false);
        feed("]");
      }
      count++;
      lexer.advance();
    }
  }

  static void feed(String s) {
    for (byte b : s.getBytes(StandardCharsets.UTF_8)) {
      hash ^= b & 0xff;
      hash *= 0x100000001b3L;
    }
  }

  static String escape(String s) {
    return s.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t");
  }
}
