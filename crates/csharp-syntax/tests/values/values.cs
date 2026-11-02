// Tokens whose ValueText is not their text, for tests/value_text.rs: the oracle's `values` lines are values.values.txt.
#define F\u004FO
#undef @x
#if F\u004FO && !BAR
#endif
#line 10 "a\b.cs"
#pragma checksum "c.cs" "{406EA660-64CF-4C82-B6F0-42D48172A799}" "ab"
#pragma warning disable CS0168, @x
#nullable enable
#region Re\u0067ion
#endregion
/// <summary>A <see cref="List{T}"/> &amp; <see cref="M{T}(T, ref int)"/> &lt;tag&gt; &#65;&#x42; &quot;q&quot;</summary>
/// <param name="@p\u0041">x</param> <typeparam name="&#84;"/> <see cref="operator &lt;"/>
/// <see cref="\u0041{\u0042}"/> <see cref="A&lt;B&gt;"/>
/** <see cref="C.op_Implicit"/> &apos; &#x1F600; */
namespace N\u0031 {
  using @static = System.Math;
  [@Obsolete("x")]
  partial class @class<@T> where @T : class {
    int @int, \u0061\U00000062, a\u00ADb;
    void M(int @params, string @event) {
      var \u0076ar = @this; v\u0061r x = nameof(@params); var q = from @from in @in select @from;
      var s = "str\t" + @"verbatim""" + $"{@int}\n" + """raw""" + 'c' + 0x10 + 1e3m;
      a\u0062c @c #line 5 "f.cs"
      ;
      int kéy = @kéy + k\u00e9y + x‍y + 😀z;
    }
  }
}
// `global` written with an entity becomes the keyword `global` of fixed text: Roslyn's tree drops the entity from here
/// <see cref="&#103;lobal::A.@class"/>
class Z { }
