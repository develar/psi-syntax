import com.intellij.lexer.Lexer;
import com.intellij.psi.tree.IElementType;
import com.intellij.openapi.util.text.StringUtil;

import org.jetbrains.bazel.languages.starlark.lexer.StarlarkIndentingLexer;

import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

public class StarlarkLexOracle {
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
    if (list == null) throw new IllegalArgumentException("--list is required");
    List<String> lines = Files.readAllLines(list, StandardCharsets.UTF_8);
    StringBuilder out = new StringBuilder();
    for (String line : lines) {
      if (line.isBlank()) continue;
      String[] columns = line.split("\t");
      String path = columns[0];
      String text;
      try {
        byte[] bytes = Files.readAllBytes(root.resolve(path));
        text = StandardCharsets.UTF_8.newDecoder()
          .onMalformedInput(CodingErrorAction.REPORT)
          .onUnmappableCharacter(CodingErrorAction.REPORT)
          .decode(ByteBuffer.wrap(bytes)).toString();
        text = StringUtil.convertLineSeparators(text);
      }
      catch (Exception e) {
        out.append(path).append("\tunreadable\t0\n");
        continue;
      }
      hash = 0xcbf29ce484222325L;
      count = 0;
      dump = dumpTokens ? new StringBuilder() : null;
      lex(new StarlarkIndentingLexer(), text);
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

  static void lex(Lexer lexer, String text) {
    lexer.start(text);
    while (true) {
      IElementType type = lexer.getTokenType();
      if (type == null) break;
      String name = type.toString();
      if (name.startsWith("Starlark:")) name = name.substring("Starlark:".length());
      String tokenText = text.substring(lexer.getTokenStart(), lexer.getTokenEnd());
      feed(name + ":" + tokenText.getBytes(StandardCharsets.UTF_8).length + ";");
      if (dump != null) dump.append("  ").append(name).append(' ').append(escape(tokenText)).append('\n');
      count++;
      lexer.advance();
    }
  }

  static void feed(String text) {
    for (byte b : text.getBytes(StandardCharsets.UTF_8)) {
      hash ^= b & 0xff;
      hash *= 0x100000001b3L;
    }
  }

  static String escape(String text) {
    return text.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t");
  }
}