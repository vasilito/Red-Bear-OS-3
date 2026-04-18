#pragma once

#if __has_include(<QOrientationReading>)
#include <QOrientationReading>
#else
class QOrientationReading
{
public:
    enum Orientation {
        TopUp,
        TopDown,
        LeftUp,
        RightUp,
        FaceUp,
        FaceDown,
        Undefined,
    };

    QOrientationReading() = default;

    Orientation orientation() const
    {
        return m_orientation;
    }

    void setOrientation(Orientation orientation)
    {
        m_orientation = orientation;
    }

private:
    Orientation m_orientation = Undefined;
};
#endif
