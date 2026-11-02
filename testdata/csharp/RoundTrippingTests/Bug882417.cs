
[AttributeUsage(AttributeTargets.Class)]
public class HelpAttribute : Attribute
{
 public HelpAttribute(byte b1) {
     b = b1;
 }
 byte b = 0;
 public byte Verify {get {return b;} }
}
