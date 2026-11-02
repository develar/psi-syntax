
public partial class Base
{
    ViolateClassConstraint Fld01 = new Base().Meth<ViolateClassConstraint>(new ViolateClassConstraint()); //E:CS0315
}
