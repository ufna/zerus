package app.zerus.mobile

import org.junit.Assert.*
import org.junit.Test

class ApprovalReviewTest {
    @Test fun threeSecondsMustBeContinuousVisibleAndAvailable() {
        var review = ApprovalReview().observe(true, 1000)
        assertEquals(3, review.secondsRemaining(1000))
        assertEquals(1, review.secondsRemaining(3999))
        assertEquals(0, review.secondsRemaining(4000))
        review = review.observe(false, 4100)
        assertEquals(3, review.secondsRemaining(10000))
        review = review.observe(true, 10000)
        assertEquals(3, review.secondsRemaining(10000))
        assertEquals(0, review.secondsRemaining(13000))
    }
    @Test fun clockReversalRestartsReview() {
        val review = ApprovalReview().observe(true, 5000).observe(true, 4000)
        assertEquals(3, review.secondsRemaining(10000))
    }
}
