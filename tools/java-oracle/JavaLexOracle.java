// The Java lexer oracle: lexes each listed file with IntelliJ's java-syntax JavaLexer (and every doc comment's text,
// `DOC_COMMENT` and `DOC_MARKDOWN_COMMENT`, with JavaDocLexer at LanguageLevel.HIGHEST, as JavaDocCommentParser does)
// and prints one line per file: "<path>\t<fnv64 hex>\t<tokens>", hashing "<debug name>:<utf-8 length>;" per token,
// the JavaDoc tokens of a doc comment following it between "[" and "]". `ksdump --lex --lang java` prints the same
// lines for our lexers, so `diff` finds every file where they disagree; --dump prints the token lists themselves.
//
//   LIB=$(tools/java-oracle/fetch-jars.sh)
//   java -cp "$LIB/*" tools/java-oracle/JavaLexOracle.java --list LIST --root ROOT [--level JDK_28] [--dump]
//
// A list line is PATH[<TAB>LEVEL...] (testdata/java/manifest.tsv is a list); LEVEL is the file's lexer level
// (default --level, itself LanguageLevel.HIGHEST). The text is read as UTF-8 with CRLF and CR converted to LF, as the
// PSI oracle does; a file that is not valid UTF-8 prints "unreadable".

import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.ByteBuffer;
import java.util.List;

import com.intellij.java.syntax.lexer.JavaDocLexer;
import com.intellij.java.syntax.lexer.JavaLexer;
import com.intellij.platform.syntax.SyntaxElementType;
import com.intellij.platform.syntax.lexer.Lexer;
import com.intellij.pom.java.LanguageLevel;

public class JavaLexOracle {
  static long hash;
  static int count;
  static StringBuilder dump;

  public static void main(String[] args) throws Exception {
    Path list = null;
    Path root = Path.of(".");
    boolean dumpTokens = false;
    LanguageLevel defaultLevel = LanguageLevel.HIGHEST;
    for (int i = 0; i < args.length; i++) {
      switch (args[i]) {
        case "--list" -> list = Path.of(args[++i]);
        case "--root" -> root = Path.of(args[++i]);
        case "--level" -> defaultLevel = LanguageLevel.valueOf(args[++i]);
        case "--dump" -> dumpTokens = true;
        default -> throw new IllegalArgumentException(args[i]);
      }
    }
    List<String> lines = Files.readAllLines(list, StandardCharsets.UTF_8);
    StringBuilder out = new StringBuilder();
    for (String line : lines) {
      if (line.isBlank()) continue;
      String[] columns = line.split("\t");
      String path = columns[0];
      LanguageLevel level = columns.length > 1 && !columns[1].isEmpty() ? LanguageLevel.valueOf(columns[1]) : defaultLevel;
      String text;
      try {
        byte[] bytes = Files.readAllBytes(root.resolve(path));
        text = StandardCharsets.UTF_8.newDecoder()
          .onMalformedInput(CodingErrorAction.REPORT)
          .onUnmappableCharacter(CodingErrorAction.REPORT)
          .decode(ByteBuffer.wrap(bytes)).toString();
      }
      catch (Exception e) {
        out.append(path).append("\tunreadable\t0\n");
        continue;
      }
      text = text.replace("\r\n", "\n").replace('\r', '\n');
      hash = 0xcbf29ce484222325L;
      count = 0;
      dump = dumpTokens ? new StringBuilder() : null;
      lex(new JavaLexer(level), text, true);
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

  static void lex(Lexer lexer, String text, boolean java) {
    lexer.start(text);
    while (true) {
      SyntaxElementType type = lexer.getTokenType();
      if (type == null) break;
      String tokenText = text.substring(lexer.getTokenStart(), lexer.getTokenEnd());
      feed(type + ":" + tokenText.getBytes(StandardCharsets.UTF_8).length + ";");
      if (dump != null) dump.append(java ? "  " : "    ").append(type).append(' ').append(escape(tokenText)).append('\n');
      String name = type.toString();
      if (java && (name.equals("DOC_COMMENT") || name.equals("DOC_MARKDOWN_COMMENT"))) {
        feed("[");
        lex(new JavaDocLexer(LanguageLevel.HIGHEST), tokenText, false);
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
