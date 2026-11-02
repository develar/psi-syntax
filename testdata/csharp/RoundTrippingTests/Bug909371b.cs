
[AttributeUsage(AttributeTargets.Class)]
public class HelpAttribute : Attribute
{
byte b = 0;
public HelpAttribute(byte b1) {
b = b1;
}
}
